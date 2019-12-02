use crate::{
    instruction::Result,
    instruction::{RuntimeError, RuntimeErrorTy},
    parser::*,
    Instruction, Options, Register, Value, NUMBER_REGISTERS,
};
use std::collections::HashMap;

#[derive(Clone)]
pub struct Scope<'a> {
    pub variables: HashMap<String, (Location, Type<'a>)>,
    pub registers: [Option<Location>; NUMBER_REGISTERS],
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Location {
    Register(Register, Option<u32>),
    Gc(u32),
}

impl<'a> Scope<'a> {
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            registers: [None; NUMBER_REGISTERS],
        }
    }
}

impl<'a> std::fmt::Debug for Scope<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scope")
            .field("variables", &self.variables)
            .finish()
    }
}

#[derive(Debug, Copy, Clone)]
pub struct InterpOptions {
    pub fault_tolerant: bool,
}

impl From<&Options> for InterpOptions {
    fn from(options: &Options) -> Self {
        Self {
            fault_tolerant: options.fault_tolerant,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Interpreter<'a> {
    pub ast: Vec<Node<'a>>,
    pub scopes: Vec<Scope<'a>>,
    pub current_scope: Scope<'a>,
    pub gc: u32,
    pub functions: HashMap<String, (Vec<Instruction>, Option<usize>)>,
    pub current_function: Vec<Instruction>,
    pub options: InterpOptions,
    pub func_index: usize,
}

impl<'a> Interpreter<'a> {
    pub fn new(ast: Vec<Node<'a>>, options: &Options) -> Self {
        Self {
            ast,
            scopes: Vec::new(),
            current_scope: Scope::new(),
            gc: 0,
            functions: HashMap::new(),
            current_function: Vec::new(),
            options: InterpOptions::from(options),
            func_index: 0,
        }
    }

    /// Interpret the contained ast and return the instructions
    pub fn interpret(mut self) -> Result<(Vec<Instruction>, Vec<Vec<Instruction>>)> {
        for node_index in 0..self.ast.len() {
            match &self.ast[node_index] {
                Node::Func(_) => {
                    // Interpret the function
                    let (name, index) = unsafe { self.interp_func(node_index)? };

                    // Will contain the newly created function
                    let mut func = Vec::new();

                    // Switch the current function and the function just created
                    std::mem::swap(&mut self.current_function, &mut func);

                    // Insert the function
                    self.functions.insert(name, (func, index));
                }

                n => {
                    error!("Top level node can not yet be interpreted: {:?}", n);
                    continue;
                }
            }
        }

        // Create fresh functions hashmap
        let mut functions = HashMap::new();

        // Swap the filled and fresh functions
        std::mem::swap(&mut self.functions, &mut functions);

        // Turn the functions into a Vector of the instructions paired with their function indices,
        // discarding the function names
        let mut functions = functions
            .into_iter()
            .map(|(_key, val)| val)
            .collect::<Vec<(Vec<Instruction>, Option<usize>)>>();

        // Sort the functions by their indices
        functions.sort_by_key(|(_, index)| *index);

        // If there are no functions, just make one with only the Halt instruction
        if functions.is_empty() {
            let functions = (vec![Instruction::Halt], Vec::<Vec<Instruction>>::new());
            trace!("Interp Output: {:?}", &functions);
            return Ok(functions);
        }

        // Get the man function from functions
        let main_func = {
            let pos = if let Some(pos) = functions.iter().position(|(_, index)| index.is_none()) {
                pos
            } else {
                return Err(RuntimeError {
                    ty: RuntimeErrorTy::MissingMain,
                    message: "Missing Main function".to_string(),
                });
            };

            let mut main = functions.remove(pos).0;
            if main.last() != Some(&Instruction::Halt) {
                main.push(Instruction::Halt);
            }
            main
        };

        // Filter out the other functions named main, will cause issues
        // FIXME: Handle extra main functions
        let functions = functions
            .into_iter()
            .filter_map(
                |(vec, index)| {
                    if index.is_some() {
                        Some(vec)
                    } else {
                        None
                    }
                },
            )
            .collect::<Vec<Vec<Instruction>>>();

        trace!("Interp Output: {:?}", (&main_func, &functions));

        Ok((main_func, functions))
    }

    pub fn reserve_reg(
        &mut self,
        location: Option<Location>,
        gc_id: Option<u32>,
    ) -> Result<Register> {
        if let Some(pos) = self
            .current_scope
            .registers
            .iter()
            .position(Option::is_none)
        {
            trace!("Found Available Register: {}", Register(pos as u8));

            if let Some(location) = location {
                self.current_scope.registers[pos] = Some(location);
            } else {
                self.current_scope.registers[pos] =
                    Some(Location::Register((pos as u8).into(), gc_id))
            }

            Ok((pos as u8).into())
        } else {
            Err(RuntimeError {
                ty: RuntimeErrorTy::CompilationError,
                message: "Failed to fetch avaliable register".to_string(),
            })
        }
    }

    /*
    unsafe fn interpret_external_file(
        &mut self,
        node_index: usize,
    ) -> Result<(Vec<Node<'a>>, Vec<codespan_reporting::diagnostic::Diagnostic>), Vec<codespan_reporting::diagnostic::Diagnostic>> {
        let import = if let Node::Import(import) = self.ast.remove(node_index) {
            import
        } else {
            unreachable!("Should be an already confirmed import");
        };

        // pub struct Import<'a> {
        //     pub file: std::path::PathBuf,
        //     pub alias: Option<Ident<'a>>,
        //     pub exposes: Exposes<'a>,
        //     pub ty: ImportType,
        // }

        if import.ty == ImportType::File {
            // Opens the imported file and reads it's contents to a string
            let contents = {
                use std::{fs::File, io::Read};

                let mut file = match File::open(&import.file) {
                    Ok(file) => file,
                    Err(err) => {
                        error!("Error opening imported file: {:?}", err);

                        return Err(RuntimeError {
                            ty: RuntimeErrorTy::FileError,
                            message: format!("The file '{}' does not exist", import.file.display()),
                        });
                    }
                };

                let mut contents = String::new();

                if let Err(err) = file.read_to_string(&mut contents) {
                    error!("Error reading imported file: {:?}", err);

                    return Err(RuntimeError {
                        ty: RuntimeErrorTy::FileError,
                        message: format!("Cannot read the file '{}'", import.file.display()),
                    });
                }

                contents
            };

            let parser = Parser::new(Some(&import.file), &contents);

            let parsed
        } else {
            unimplemented!(
                "The import type {:?} has not been implemented yet",
                import.ty
            );
        }
    }
    */

    /// Interpret a function
    unsafe fn interp_func(&mut self, node_index: usize) -> Result<(String, Option<usize>)> {
        let func = if let Node::Func(func) = self.ast.remove(node_index) {
            func
        } else {
            unreachable!("Should be an already a confirmed function");
        };

        // For each expression in the function, evaluate it into instructions
        for expr in func.body {
            match expr.expr {
                // Bind a variable to a value
                FuncExpr::Binding(binding) => {
                    match binding.val {
                        BindingVal::Literal(literal) => {
                            let value = literal.val.into();

                            // Insert the variable into the gc
                            let id = self.get_next_gc_id();
                            let addr = self.reserve_reg(None, Some(id))?;

                            // Add the variable to the current scope
                            self.current_scope.variables.insert(
                                binding.name.name.to_string(),
                                (Location::Gc(id), binding.ty.clone()),
                            );

                            // Add the cache instruction to the current function
                            self.add_to_current(&[Instruction::Cache(id, value, addr)]);
                        }

                        BindingVal::BinOp(bin_op) => match (bin_op.left, bin_op.right) {
                            (BinOpSide::Literal(left), BinOpSide::Literal(right)) => {
                                let left: Value = left.val.into();
                                let right = right.val.into();
                                let addr = self.reserve_reg(None, None)?;

                                // Do the math here, instead of at runtime
                                self.add_to_current(&[Instruction::Load((left + right)?, addr)]);

                                self.current_scope.variables.insert(
                                    binding.name.name.to_string(),
                                    (Location::Register(addr, None), binding.ty.clone()),
                                );
                            }

                            (BinOpSide::Literal(left), BinOpSide::Variable(right)) => {
                                let left_value = left.val.into();
                                let left_addr = self.reserve_reg(None, None)?;

                                self.add_to_current(&[Instruction::Load(left_value, left_addr)]);

                                let (right_id, faulted) = {
                                    if let Some(var) =
                                        self.current_scope.variables.get(&*right.name)
                                    {
                                        (var.0, false)
                                    } else {
                                        // If fault tolerant, a fault just occurred, so pretend it didn't
                                        if self.options.fault_tolerant {
                                            let gc_id = self.get_next_gc_id();
                                            let addr = self.reserve_reg(None, Some(gc_id))?;

                                            self.add_to_current(&[Instruction::Cache(
                                                gc_id,
                                                Value::None,
                                                addr,
                                            )]);

                                            (Location::Gc(gc_id), true)

                                        // If we aren't fault tolerant, scream
                                        } else {
                                            return Err(RuntimeError {
                                                ty: RuntimeErrorTy::NullVar,
                                                message: format!(
                                                    "The variable {:?} does not exist",
                                                    &*right.name
                                                ),
                                            });
                                        }
                                    }
                                };

                                if let Some(right_addr) = self
                                    .current_scope
                                    .registers
                                    .iter()
                                    .position(|reg| *reg == Some(right_id))
                                {
                                    let right_addr = (right_addr as u8).into();

                                    self.add_to_current(&[Instruction::Add(left_addr, right_addr)]);

                                    if self.options.fault_tolerant && faulted {
                                        self.add_to_current(&[Instruction::DropReg(right_addr)]);

                                        self.current_scope.registers[*right_addr as usize] = None;
                                        self.current_scope.registers[*left_addr as usize] = None;
                                    }
                                } else {
                                    let right_addr = self.reserve_reg(Some(right_id), None)?;

                                    self.add_to_current(&[Instruction::Add(left_addr, right_addr)]);

                                    self.current_scope.registers[*right_addr as usize] = None;
                                    self.current_scope.registers[*left_addr as usize] = None;
                                }

                                let output = self.reserve_reg(None, None)?;
                                self.add_to_current(&[
                                    Instruction::DropReg(left_addr),
                                    Instruction::OpToReg(output),
                                ]);

                                self.current_scope.variables.insert(
                                    binding.name.name.to_string(),
                                    (Location::Register(output, None), binding.ty.clone()),
                                );
                            }

                            (BinOpSide::Variable(left), BinOpSide::Literal(right)) => {
                                let (left_id, faulted) = {
                                    if let Some(var) = self.current_scope.variables.get(&*left.name)
                                    {
                                        (var.0, false)
                                    } else {
                                        // If fault tolerant, a fault just occurred, so pretend it didn't
                                        if self.options.fault_tolerant {
                                            let gc_id = self.get_next_gc_id();
                                            let addr = self.reserve_reg(None, Some(gc_id))?;

                                            self.add_to_current(&[Instruction::Cache(
                                                gc_id,
                                                Value::None,
                                                addr,
                                            )]);

                                            (Location::Gc(gc_id), true)

                                        // If we aren't fault tolerant, scream
                                        } else {
                                            return Err(RuntimeError {
                                                ty: RuntimeErrorTy::NullVar,
                                                message: format!(
                                                    "The variable {:?} does not exist",
                                                    &*left.name
                                                ),
                                            });
                                        }
                                    }
                                };

                                let (right, right_id) = (right.val.into(), self.get_next_gc_id());
                                let right_addr = self.reserve_reg(None, Some(right_id))?;

                                self.add_to_current(&[Instruction::Cache(
                                    right_id, right, right_addr,
                                )]);

                                if let Some(left_addr) = self
                                    .current_scope
                                    .registers
                                    .iter()
                                    .position(|reg| *reg == Some(left_id))
                                {
                                    let left_addr = (left_addr as u8).into();

                                    self.add_to_current(&[Instruction::Add(left_addr, right_addr)]);

                                    if self.options.fault_tolerant && faulted {
                                        self.add_to_current(&[Instruction::DropReg(right_addr)]);

                                        self.current_scope.registers[*right_addr as usize] = None;
                                        self.current_scope.registers[*left_addr as usize] = None;
                                    }
                                } else {
                                    let left_addr = self.reserve_reg(Some(left_id), None)?;

                                    self.add_to_current(&[Instruction::Add(left_addr, right_addr)]);

                                    self.current_scope.registers[*right_addr as usize] = None;
                                    self.current_scope.registers[*left_addr as usize] = None;
                                }

                                let output = self.reserve_reg(None, None)?;
                                self.add_to_current(&[
                                    Instruction::DropReg(right_addr),
                                    Instruction::Drop(right_id),
                                    Instruction::OpToReg(output),
                                ]);

                                self.current_scope.variables.insert(
                                    binding.name.name.to_string(),
                                    (Location::Register(output, None), binding.ty.clone()),
                                );
                            }
                            _ => unimplemented!(),
                        },
                        _ => unimplemented!(),
                    }
                }

                // Compiler builtin functions
                FuncExpr::Builtin(builtin) => match builtin {
                    // GC Collection cycle
                    Builtin::Collect => {
                        self.add_to_current(&[Instruction::Collect]);
                    }

                    // Halt execution
                    Builtin::Halt => {
                        self.add_to_current(&[Instruction::Halt]);
                    }

                    Builtin::SyscallExit(_exit_code) => {
                        unimplemented!("Syscalls have not been implemented");

                        #[allow(unreachable_code)]
                        match _exit_code {
                            // For literals fed into the print function, load them, print them, and drop them
                            IdentLiteral::Literal(literal) => {
                                let (val, gc_id) = (literal.val.into(), self.get_next_gc_id());

                                let reg_addr = self.reserve_reg(Some(Location::Gc(gc_id)), None)?;

                                self.add_to_current(&[
                                    Instruction::Cache(gc_id, val, reg_addr),
                                    Instruction::Print(reg_addr),
                                    Instruction::DropReg(reg_addr),
                                    Instruction::Drop(gc_id),
                                ]);

                                self.current_scope.registers[*reg_addr as usize] = None;
                            }

                            // For existing variables, fetch them and print them
                            IdentLiteral::Variable(ident) => {
                                // Get the gc address, variable type, and fault status of fetching the requested variable
                                // If a fault occurs and fault_tolerant is true, then a null value will be used in place of
                                // the requested variable. If fault_tolerant is false, then a Runtime Error will be thrown
                                let (var_id, _var_type, faulted) = if let Some(var) =
                                    self.current_scope.variables.get(&*ident.name)
                                {
                                    (var.0, var.1.clone(), false)
                                } else {
                                    // If fault tolerant, a fault just occurred, so pretend it didn't
                                    if self.options.fault_tolerant {
                                        let gc_id = self.get_next_gc_id();
                                        let addr = self.reserve_reg(None, Some(gc_id))?;

                                        self.add_to_current(&[Instruction::Cache(
                                            gc_id,
                                            Value::None,
                                            addr,
                                        )]);

                                        (Location::Gc(gc_id), Type::Void, true)

                                    // If we aren't fault tolerant, scream
                                    } else {
                                        return Err(RuntimeError {
                                            ty: RuntimeErrorTy::NullVar,
                                            message: format!(
                                                "The variable {:?} does not exist",
                                                &*ident.name
                                            ),
                                        });
                                    }
                                };

                                // If the value is currently loaded into a register, print it directly
                                if let Some(reg_addr) =
                                    self.current_scope.registers.iter().position(|reg| {
                                        *reg == Some(var_id) || {
                                            if let Some(Location::Register(_, loc)) = *reg {
                                                if let Location::Gc(__loc) = var_id {
                                                    loc == Some(__loc)
                                                } else {
                                                    false
                                                }
                                            } else {
                                                false
                                            }
                                        }
                                    })
                                {
                                    let reg_addr = (reg_addr as u8).into();
                                    self.add_to_current(&[Instruction::Print(reg_addr)]);

                                    // Note: If the value was already loaded, we probably don't want to drop it yet

                                    // If the value was magically loaded in fault-tolerant mode with a fault,
                                    // drop it from the registers
                                    if self.options.fault_tolerant && faulted {
                                        self.add_to_current(&[Instruction::DropReg(reg_addr)]);

                                        self.current_scope.registers[*reg_addr as usize] = None;
                                    }

                                // If the value is not currently loaded, load, print, and drop it from the registers
                                } else {
                                    let reg_addr = self.reserve_reg(Some(var_id), None)?;

                                    self.add_to_current(&[
                                        // Instruction::Syscall(reg_addr),
                                        Instruction::DropReg(reg_addr),
                                    ]);

                                    self.current_scope.registers[*reg_addr as usize] = None;
                                }

                                // Drop the null value from the registers if a fault occurred
                                if self.options.fault_tolerant && faulted {
                                    self.add_to_current(&[Instruction::Drop(
                                        if let Location::Gc(loc) = var_id {
                                            loc
                                        } else {
                                            unimplemented!()
                                        },
                                    )]);
                                }
                            }
                        }
                    }

                    // Print values
                    Builtin::Print(params) => {
                        for param in params {
                            match param {
                                // For literals fed into the print function, load them, print them, and drop them
                                IdentLiteral::Literal(literal) => {
                                    let (val, reg_addr) =
                                        (literal.val.into(), self.reserve_reg(None, None)?);

                                    // Literals can just be moved into a register
                                    self.add_to_current(&[
                                        Instruction::Load(val, reg_addr),
                                        Instruction::Print(reg_addr),
                                        Instruction::DropReg(reg_addr),
                                    ]);

                                    self.current_scope.registers[*reg_addr as usize] = None;
                                }

                                // For existing variables, fetch them and print them
                                IdentLiteral::Variable(ident) => {
                                    // Get the gc address, variable type, and fault status of fetching the requested variable
                                    // If a fault occurs and fault_tolerant is true, then a null value will be used in place of
                                    // the requested variable. If fault_tolerant is false, then a Runtime Error will be thrown
                                    let (var_id, _var_type, faulted) = if let Some(var) =
                                        self.current_scope.variables.get(&*ident.name)
                                    {
                                        (var.0, var.1.clone(), false)
                                    } else {
                                        // If fault tolerant, a fault just occurred, so pretend it didn't
                                        if self.options.fault_tolerant {
                                            let gc_id = self.get_next_gc_id();
                                            let addr = self.reserve_reg(None, Some(gc_id))?;

                                            self.add_to_current(&[Instruction::Cache(
                                                gc_id,
                                                Value::None,
                                                addr,
                                            )]);

                                            (Location::Gc(gc_id), Type::Void, true)

                                        // If we aren't fault tolerant, scream
                                        } else {
                                            return Err(RuntimeError {
                                                ty: RuntimeErrorTy::NullVar,
                                                message: format!(
                                                    "The variable {:?} does not exist",
                                                    &*ident.name
                                                ),
                                            });
                                        }
                                    };

                                    // If the value is currently loaded into a register, print it directly
                                    if let Some(reg_addr) =
                                        self.current_scope.registers.iter().position(|reg| {
                                            *reg == Some(var_id) || {
                                                if let Some(Location::Register(_, loc)) = *reg {
                                                    if let Location::Gc(__loc) = var_id {
                                                        loc == Some(__loc)
                                                    } else {
                                                        false
                                                    }
                                                } else {
                                                    false
                                                }
                                            }
                                        })
                                    {
                                        let reg_addr = (reg_addr as u8).into();
                                        self.add_to_current(&[Instruction::Print(reg_addr)]);

                                        // Note: If the value was already loaded, we probably don't want to drop it yet

                                        // If the value was magically loaded in fault-tolerant mode with a fault,
                                        // drop it from the registers
                                        if self.options.fault_tolerant && faulted {
                                            self.add_to_current(&[Instruction::DropReg(reg_addr)]);

                                            self.current_scope.registers[*reg_addr as usize] = None;
                                        }

                                    // If the value is not currently loaded, load, print, and drop it from the registers
                                    } else {
                                        let reg_addr = self.reserve_reg(Some(var_id), None)?;

                                        self.add_to_current(&[
                                            Instruction::Print(reg_addr),
                                            Instruction::DropReg(reg_addr),
                                        ]);

                                        self.current_scope.registers[*reg_addr as usize] = None;
                                    }

                                    // Drop the null value from the registers if a fault occurred
                                    if self.options.fault_tolerant && faulted {
                                        self.add_to_current(&[Instruction::Drop(
                                            if let Location::Gc(loc) = var_id {
                                                loc
                                            } else {
                                                unimplemented!()
                                            },
                                        )]);
                                    }
                                }
                            }
                        }
                    }
                },

                // FuncExpr::Assign(assign) => {
                //     // pub struct Assign<'a> {
                //     //     pub name: Ident<'a>,
                //     //     pub val: IdentLiteral<'a>,
                //     //     pub info: LocInfo,
                //     // }
                //
                //     // variables: HashMap<String, (Location, Type<'a>)>,
                //
                //     let var = if let Some(var) =
                //         self.current_scope.variables.get(&*assign.name.name)
                //     {
                //         var
                //     } else {
                //         return Err(RuntimeError {
                //             ty: RuntimeErrorTy::MissingValue,
                //             message: "The variable being assigned to does not exist".to_string(),
                //         });
                //     };
                //
                //     let reg = self.reserve_reg(Some(var.0), None);
                //
                //     self.add_to_current(&[Instruction::Load( )]);
                // }
                FuncExpr::FuncCall(_) | FuncExpr::Assign(_) => unimplemented!(),
            }
        }

        // Enter a new scope for the next function
        self.enter_scope();

        let index = match &*func.name.name {
            "main" => None,
            _ => Some(self.get_next_func_id()),
        };

        Ok((func.name.name.to_string(), index))
    }

    fn add_to_current(&mut self, instructions: &[Instruction]) {
        self.current_function.extend_from_slice(instructions);
        trace!("Adding Instructions to function: {:?}", instructions);
    }

    pub fn get_next_gc_id(&mut self) -> u32 {
        let id = self.gc;
        trace!("Got GC ID: {}", id);
        self.gc += 1;
        id
    }

    pub fn get_next_func_id(&mut self) -> usize {
        self.func_index += 1;
        self.func_index
    }

    /// Enters a new scope
    fn enter_scope(&mut self) {
        let mut new = Scope::new();
        std::mem::swap(&mut new, &mut self.current_scope);
        self.scopes.push(new);
    }
}
