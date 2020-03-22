mod options;
#[cfg(test)]
mod tests;

pub use options::ViceOptions;

use crate::{
    code_builder::{Block, CodeBuilder, FunctionContext},
    data_location::DataLocation,
    intrinsics::INTRINSICS,
};

use compactor::{bytecode, Instruction, Value};
use crunch_error::compile_prelude::*;
use crunch_parser::{
    ast::*,
    string_interner::{StringInterner, Sym},
};

// Note: For no_std, convert to BTreeMap and use a custom Path-like type (Maybe String)
use std::{collections::HashMap, path::PathBuf};

// TODO: Access control this somehow, all pub is bad
/// The Crunch compiler
#[derive(Debug, Clone)]
pub struct Vice {
    pub gc: u32,
    pub functions: HashMap<Sym, (Vec<Instruction>, Option<usize>)>,
    pub current_function: Vec<Instruction>,
    pub options: ViceOptions,
    pub func_index: usize,
    pub builder: CodeBuilder,
}

impl Vice {
    #[allow(dead_code)]
    pub fn new(options: ViceOptions) -> Self {
        Self {
            gc: 0,
            functions: HashMap::new(),
            current_function: Vec::new(),
            options,
            func_index: 0,
            builder: CodeBuilder::new(),
        }
    }

    // TODO: Default from interner of some sort
    pub fn from_interner(options: ViceOptions, interner: StringInterner<Sym>) -> Self {
        Self {
            gc: 0,
            functions: HashMap::new(),
            current_function: Vec::new(),
            options,
            func_index: 0,
            builder: CodeBuilder::from_interner(interner),
        }
    }

    /// Interpret the contained ast and return the instructions
    pub fn compile(mut self, ast: Vec<Program>) -> CompileResult<Vec<Vec<Instruction>>> {
        self.interpret_module(ast)?;
        let functions = self.builder.build()?;

        trace!("Interp Output: {:?}", functions);

        Ok(functions)
    }

    fn interpret_module(&mut self, mut ast: Vec<Program>) -> CompileResult<()> {
        while let Some(node) = ast.pop() {
            match node {
                Program::FunctionDecl(func) => {
                    // Interpret the function
                    let (name, index) = self.interp_func(func)?;

                    // Will contain the newly created function
                    let mut func = Vec::new();

                    // Switch the current function and the function just created
                    std::mem::swap(&mut self.current_function, &mut func);

                    // Insert the function
                    self.functions.insert(name, (func, index));
                }

                Program::Import(import) => {
                    self.interpret_import(import)?;
                }

                Program::TypeDecl(_ty) => todo!(),
            }
        }

        Ok(())
    }

    fn interpret_import(&mut self, import: Import) -> CompileResult<()> {
        match import.source {
            ImportSource::File(relative_path) => {
                // TODO: allow importing folders

                let contents = {
                    use std::{fs::File, io::Read};

                    let mut path = PathBuf::from("./");
                    path.push(&relative_path);

                    let mut file = match File::open(&path.with_extension("crunch")) {
                        Ok(file) => file,
                        Err(err) => {
                            error!("Error opening imported file: {:?}", err);

                            return Err(CompileError::new(
                                CompileErrorTy::FileError,
                                format!("The file '{}' does not exist", relative_path.display()),
                            ));
                        }
                    };

                    let mut contents = String::new();

                    if let Err(err) = file.read_to_string(&mut contents) {
                        error!("Error reading imported file: {:?}", err);

                        return Err(CompileError::new(
                            CompileErrorTy::FileError,
                            format!("Cannot read the file '{}'", relative_path.display()),
                        ));
                    }

                    contents
                };

                let file_name = relative_path.to_string_lossy();
                todo!("What is going on here");
                // let import_ast =
                //     match crunch_parser::Parser::new(Some(&*file_name), &contents).parse() {
                //         Ok((ast, _diagnostics)) => {
                //             // TODO: Emit errors
                //             ast
                //         }
                //         Err(_err) => {
                //             // TODO: Emit errors
                //             return Err(CompileError::new(
                //                 CompileErrorTy::CompilationError,
                //                 format!("The dependency '{}' failed to compile", file_name),
                //             ));
                //         }
                //     };
                //
                // self.interpret_module(import_ast)?;
            }
            ImportSource::Package(_sym) => todo!("Package code loading"),
            ImportSource::Native(_sym) => todo!("Native code loading"),
        }

        Ok(())
    }

    /// Interpret a function
    fn interp_func(&mut self, func: FunctionDecl) -> CompileResult<(Sym, Option<usize>)> {
        let mut builder = CodeBuilder::new();
        std::mem::swap(&mut builder, &mut self.builder);

        let func_name = func.name;
        builder.build_function(func_name, |builder, ctx| {
            for (arg, _ty) in func.arguments {
                let loc = ctx.reserve_reg(arg)?;
                bytecode!(@append ctx.current_block() => {
                    pop loc;
                });
            }

            // For each expression in the function, evaluate it into instructions
            for statement in func.body {
                self.statement(statement, builder, ctx)?;
            }

            Ok(())
        })?;

        std::mem::swap(&mut builder, &mut self.builder);
        drop(builder);

        let index = match self.builder.interner.resolve(func_name) {
            Some("main") => None,
            _ => Some(self.get_next_func_id()),
        };

        Ok((func_name, index))
    }

    pub fn get_next_func_id(&mut self) -> usize {
        self.func_index += 1;
        self.func_index
    }

    fn expr(
        &mut self,
        builder: &mut CodeBuilder,
        ctx: &mut FunctionContext,
        expr: Expr,
        target: impl Into<Option<Sym>>,
    ) -> CompileResult<DataLocation> {
        match expr {
            Expr::Literal(literal) => {
                let addr = ctx.reserve_reg(target)?;

                let value = match literal {
                    Literal::String(sym) => Value::Str(Box::leak(
                        builder
                            .interner
                            .resolve(sym)
                            .unwrap()
                            .to_string()
                            .into_boxed_str(),
                    )),
                    Literal::Integer(int) => Value::I32(int),
                    Literal::Boolean(boolean) => Value::Bool(boolean),
                };

                bytecode!(@append ctx.current_block() => {
                    load value, addr;
                });

                Ok(DataLocation::Register(addr))
            }
            Expr::Range(_range) => todo!("What even do I do here?"),
            Expr::Comparison(comparison) => {
                let (left, right) = (
                    self.expr(builder, ctx, *comparison.left, None)?
                        .to_register(ctx, None)?,
                    self.expr(builder, ctx, *comparison.right, None)?
                        .to_register(ctx, None)?,
                );

                match comparison.comparison {
                    Comparator::Equal => ctx.current_block().inst_eq(left, right),
                    Comparator::NotEqual => ctx.current_block().inst_not_eq(left, right),
                    Comparator::LessEqual => ctx.current_block().inst_less_than_eq(left, right),
                    Comparator::GreaterEqual => {
                        ctx.current_block().inst_greater_than_eq(left, right)
                    }
                    Comparator::Less => ctx.current_block().inst_less_than(left, right),
                    Comparator::Greater => ctx.current_block().inst_greater_than(left, right),
                };

                Ok(DataLocation::Comparison)
            }
            Expr::BinaryOperation(bin_op) => {
                let (left, right) = (
                    self.expr(builder, ctx, *bin_op.left, None)?
                        .to_register(ctx, None)?,
                    self.expr(builder, ctx, *bin_op.right, None)?
                        .to_register(ctx, None)?,
                );
                let output = ctx.reserve_reg(target)?;

                // TODO: Handle different operation types
                match bin_op.op {
                    (BinaryOp::Plus, _ty) => ctx.current_block().inst_add(left, right),
                    (BinaryOp::Minus, _ty) => ctx.current_block().inst_sub(left, right),
                    (BinaryOp::Mult, _ty) => ctx.current_block().inst_mult(left, right),
                    (BinaryOp::Div, _ty) => ctx.current_block().inst_div(left, right),
                    (BinaryOp::And, _ty) => ctx.current_block().inst_and(left, right),
                    (BinaryOp::Or, _ty) => ctx.current_block().inst_or(left, right),
                    (BinaryOp::Xor, _ty) => ctx.current_block().inst_xor(left, right),
                };

                bytecode!(@append ctx.current_block() => {
                    opr output;
                });

                Ok(DataLocation::Register(output))
            }
            Expr::Ident(sym) => Ok(DataLocation::Register(ctx.get_cached_reg(sym).map_err(
                |err| {
                    dbg!(builder.interner.resolve(sym));
                    err
                },
            )?)),
            Expr::Expr(expr) => self.expr(builder, ctx, *expr, target),
            Expr::Array(arr) => {
                ctx.add_block();
                let array = ctx.reserve_reg(target)?;

                for expr in arr {
                    let expr = self
                        .expr(builder, ctx, expr, None)?
                        .to_register(ctx, None)?;
                    ctx.inst_push_arr(array, expr, ctx.current_block);
                }
                ctx.add_block();

                Ok(DataLocation::Register(array))
            }
            Expr::FunctionCall(func_call) => {
                for arg in func_call.arguments {
                    let reg = self.expr(builder, ctx, arg, None)?.to_register(ctx, None)?;

                    ctx.current_block().inst_push(reg);
                    ctx.free_reg(reg);
                }

                let mut intrinsic_fn = false;
                if let Some(abs_path) = builder.interner.resolve(func_call.name) {
                    if let Some(intrinsic) = INTRINSICS.get(abs_path) {
                        (intrinsic)(builder, ctx)?;
                        intrinsic_fn = true;
                    }
                }

                let ret_val = ctx.reserve_reg(None)?;
                if !intrinsic_fn {
                    ctx.current_block()
                        .inst_func_call(func_call.name)
                        .inst_pop(ret_val);
                }

                Ok(DataLocation::Register(ret_val))
            }
        }
    }

    fn statement(
        &mut self,
        statement: Statement,
        builder: &mut CodeBuilder,
        ctx: &mut FunctionContext,
    ) -> CompileResult<()> {
        match statement {
            Statement::Assign(assign) => {
                // TODO: If type is not copyable, move it or clone it

                let reg = ctx.get_cached_reg(assign.var)?;
                let loaded = self
                    .expr(builder, ctx, assign.expr, None)?
                    .to_register(ctx, None)?;

                if assign.ty == AssignType::Normal {
                    ctx.current_block().inst_copy(loaded, reg);
                } else if let AssignType::BinaryOp(op) = assign.ty {
                    match op {
                        BinaryOp::Plus => ctx.current_block().inst_add(loaded, reg),
                        BinaryOp::Minus => ctx.current_block().inst_sub(loaded, reg),
                        BinaryOp::Mult => ctx.current_block().inst_mult(loaded, reg),
                        BinaryOp::Div => ctx.current_block().inst_div(loaded, reg),
                        BinaryOp::Xor => ctx.current_block().inst_xor(loaded, reg),
                        BinaryOp::Or => ctx.current_block().inst_or(loaded, reg),
                        BinaryOp::And => ctx.current_block().inst_and(loaded, reg),
                    }
                    .inst_op_to_reg(reg);
                } else {
                    unreachable!()
                }
            }

            Statement::While(_while_loop) => todo!("Compile While loops"),
            Statement::Loop(_loop_loop) => todo!("Compile Loops"),
            Statement::For(for_loop) => {
                // TODO: Make this general-purpose
                if let Expr::Range(range) = for_loop.range {
                    let start = self
                        .expr(builder, ctx, *range.start, for_loop.element)?
                        .to_register(ctx, for_loop.element)?;
                    let end = self
                        .expr(builder, ctx, *range.end, None)?
                        .to_register(ctx, None)?;
                    let one = ctx.reserve_reg(None)?;
                    bytecode!(@append ctx.current_block() => {
                        load 1i32, one;
                    });

                    ctx.add_block();
                    let block = ctx.current_block;

                    for statement in for_loop.body {
                        self.statement(statement, builder, ctx)?;
                    }

                    ctx.add_block();
                    bytecode!(@append ctx.current_block() => {
                        add start, one;
                        opr start;
                        print start;
                        less start, end;
                        jumpcmp block as i32;
                    });

                    bytecode!(@append ctx.current_block() => {
                        drop one;
                        drop start;
                        drop end;
                    });
                    ctx.add_block();
                } else {
                    todo!("Other range types")
                }
            }

            Statement::VarDecl(var_decl) => {
                self.expr(builder, ctx, var_decl.expr, var_decl.name)?
                    .to_register(ctx, var_decl.name)?;
            }

            Statement::Return(Return { expr, .. }) => {
                if let Some(ret) = expr {
                    let loaded = self.expr(builder, ctx, ret, None)?.to_register(ctx, None)?;

                    bytecode!(@append ctx.current_block() => {
                        push loaded;
                    });
                }

                bytecode!(@append ctx.current_block() => {
                    ret;
                });

                ctx.add_block();
            }
            Statement::Continue => todo!("Compile Continue statements"),
            Statement::Break => todo!("Compile break statements"),
            Statement::Expr(expr) => {
                self.expr(builder, ctx, expr, None)?;
            }
            Statement::Empty => { /* Do nothing for `empty` */ }

            Statement::Conditional(conditional) => {
                ctx.add_block();
                let conditional_block = ctx.current_block;

                let mut conditions = Vec::with_capacity(conditional._if.len() + 1);
                for If {
                    condition, body, ..
                } in conditional._if
                {
                    ctx.move_to_block(conditional_block);
                    self.expr(builder, ctx, condition, None)?;

                    ctx.add_block();
                    let if_block = ctx.current_block;
                    conditions.push(if_block);

                    ctx.move_to_block(conditional_block);
                    ctx.current_block().inst_jump_comp(if_block as u32);

                    ctx.move_to_block(if_block);
                    for statement in body {
                        self.statement(statement, builder, ctx)?;
                    }
                }

                if let Some(Else { body, .. }) = conditional._else {
                    ctx.add_block();
                    let else_block = ctx.current_block;
                    conditions.push(else_block);

                    ctx.move_to_block(conditional_block);
                    ctx.current_block().inst_jump(else_block as u32);

                    ctx.move_to_block(else_block);
                    for statement in body {
                        self.statement(statement, builder, ctx)?;
                    }
                }

                ctx.add_block();
                let after_block = ctx.current_block;
                for block in conditions {
                    ctx.get_block(block).inst_jump(after_block as u32);
                }
            }
        }

        Ok(())
    }
}

impl Default for Vice {
    fn default() -> Self {
        Self::new(ViceOptions::default())
    }
}