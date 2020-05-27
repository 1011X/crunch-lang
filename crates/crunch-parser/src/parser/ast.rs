use crate::{
    context::StrT,
    error::{Error, Locatable, Location, ParseResult, Span, SyntaxError},
    parser::{CurrentFile, Expr, Literal, Parser, Stmt, Type},
    token::{Token, TokenType},
};

use alloc::{format, string::ToString, vec::Vec};
use core::{convert::TryFrom, mem};
use crunch_proc::recursion_guard;
#[cfg(test)]
use serde::Serialize;
use stadium::Ticket;

// TODO: Const blocks
// TODO: Add back generics to funcs

#[cfg_attr(test, derive(Serialize))]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Function<'ctx> {
    pub decorators: Vec<Locatable<Decorator<'ctx>>>,
    pub attrs: Vec<Locatable<Attribute>>,
    pub name: StrT,
    pub args: Vec<Locatable<FuncArg<'ctx>>>,
    pub returns: Locatable<Ticket<'ctx, Type<'ctx>>>,
    pub body: Vec<Ticket<'ctx, Stmt<'ctx>>>,
}

#[cfg_attr(test, derive(Serialize))]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FuncArg<'ctx> {
    pub name: Locatable<StrT>,
    pub ty: Locatable<Ticket<'ctx, Type<'ctx>>>,
    pub comptime: bool,
}

#[cfg_attr(test, derive(Serialize))]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeDecl<'ctx> {
    pub decorators: Vec<Locatable<Decorator<'ctx>>>,
    pub attrs: Vec<Locatable<Attribute>>,
    pub name: StrT,
    pub generics: Vec<Locatable<Ticket<'ctx, Type<'ctx>>>>,
    pub members: Vec<Locatable<TypeMember<'ctx>>>,
}

#[cfg_attr(test, derive(Serialize))]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeMember<'ctx> {
    pub decorators: Vec<Locatable<Decorator<'ctx>>>,
    pub attrs: Vec<Locatable<Attribute>>,
    pub name: StrT,
    pub ty: Locatable<Ticket<'ctx, Type<'ctx>>>,
}

#[cfg_attr(test, derive(Serialize))]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Enum<'ctx> {
    pub decorators: Vec<Locatable<Decorator<'ctx>>>,
    pub attrs: Vec<Locatable<Attribute>>,
    pub name: StrT,
    pub generics: Vec<Locatable<Ticket<'ctx, Type<'ctx>>>>,
    pub variants: Vec<Locatable<EnumVariant<'ctx>>>,
}

#[cfg_attr(test, derive(Serialize))]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EnumVariant<'ctx> {
    Unit {
        name: StrT,
        decorators: Vec<Locatable<Decorator<'ctx>>>,
    },

    Tuple {
        name: StrT,
        elements: Vec<Locatable<Ticket<'ctx, Type<'ctx>>>>,
        decorators: Vec<Locatable<Decorator<'ctx>>>,
    },
}

impl<'ctx> EnumVariant<'ctx> {
    pub fn name(&self) -> StrT {
        match self {
            Self::Unit { name, .. } => *name,
            Self::Tuple { name, .. } => *name,
        }
    }
}

#[cfg_attr(test, derive(Serialize))]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Trait<'ctx> {
    pub decorators: Vec<Locatable<Decorator<'ctx>>>,
    pub attrs: Vec<Locatable<Attribute>>,
    pub name: StrT,
    pub generics: Vec<Locatable<Ticket<'ctx, Type<'ctx>>>>,
    pub methods: Vec<Locatable<Function<'ctx>>>,
}

#[cfg_attr(test, derive(Serialize))]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Import {
    pub file: Locatable<StrT>,
    pub dest: ImportDest,
    pub exposes: ImportExposure,
}

#[cfg_attr(test, derive(Serialize))]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExtendBlock<'ctx> {
    pub target: Locatable<Ticket<'ctx, Type<'ctx>>>,
    pub extender: Option<Locatable<Ticket<'ctx, Type<'ctx>>>>,
    pub nodes: Vec<Ticket<'ctx, Ast<'ctx>>>,
}

#[cfg_attr(test, derive(Serialize))]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Alias<'ctx> {
    pub decorators: Vec<Locatable<Decorator<'ctx>>>,
    pub attrs: Vec<Locatable<Attribute>>,
    pub alias: Locatable<Ticket<'ctx, Type<'ctx>>>,
    pub actual: Locatable<Ticket<'ctx, Type<'ctx>>>,
}

#[cfg_attr(test, derive(Serialize))]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Ast<'ctx> {
    Function(Locatable<Function<'ctx>>),
    Type(Locatable<TypeDecl<'ctx>>),
    Enum(Locatable<Enum<'ctx>>),
    Trait(Locatable<Trait<'ctx>>),
    Import(Locatable<Import>),
    ExtendBlock(Locatable<ExtendBlock<'ctx>>),
    Alias(Locatable<Alias<'ctx>>),
}

impl<'ctx> Ast<'ctx> {
    pub fn name(&self) -> Option<StrT> {
        match self {
            Self::Function(func) => Some(func.name),
            Self::Type(ty) => Some(ty.name),
            Self::Enum(e) => Some(e.name),
            Self::Trait(tr) => Some(tr.name),
            Self::Import(..) | Self::ExtendBlock(..) | Self::Alias(..) => None,
        }
    }

    pub fn is_import(&self) -> bool {
        if let Self::Import { .. } = self {
            true
        } else {
            false
        }
    }

    pub fn is_function(&self) -> bool {
        if let Self::Function { .. } = self {
            true
        } else {
            false
        }
    }

    pub fn location(&self) -> Location {
        match self {
            Self::Function(func) => func.loc(),
            Self::Type(ty) => ty.loc(),
            Self::Enum(en) => en.loc(),
            Self::Trait(tr) => tr.loc(),
            Self::Import(import) => import.loc(),
            Self::ExtendBlock(block) => block.loc(),
            Self::Alias(alias) => alias.loc(),
        }
    }
}

#[cfg_attr(test, derive(Serialize))]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ImportExposure {
    None(Locatable<StrT>),
    All,
    Members(Vec<Locatable<(StrT, Option<StrT>)>>),
}

#[cfg_attr(test, derive(Serialize))]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum ImportDest {
    NativeLib,
    Package,
    Relative,
}

impl Default for ImportDest {
    fn default() -> Self {
        Self::Relative
    }
}

#[cfg_attr(test, derive(Serialize))]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Decorator<'ctx> {
    pub name: Locatable<StrT>,
    pub args: Vec<Ticket<'ctx, Expr<'ctx>>>,
}

#[cfg_attr(test, derive(Serialize))]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Attribute {
    Visibility(Visibility),
    Const,
}

impl Attribute {
    #[inline]
    pub fn is_visibility(self) -> bool {
        if let Self::Visibility(_) = self {
            true
        } else {
            false
        }
    }

    #[inline]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Visibility(vis) => vis.as_str(),
            Self::Const => "const",
        }
    }
}

impl<'a> TryFrom<(&Token<'a>, CurrentFile)> for Attribute {
    type Error = Locatable<Error>;

    fn try_from((token, file): (&Token<'a>, CurrentFile)) -> Result<Self, Self::Error> {
        Ok(match token.ty() {
            TokenType::Exposed => Self::Visibility(Visibility::Exposed),
            TokenType::Package => Self::Visibility(Visibility::Package),
            TokenType::Const => Self::Const,

            _ => {
                return Err(Locatable::new(
                    Error::Syntax(SyntaxError::Generic(format!(
                        "Expected an attribute, got `{}`",
                        token.ty()
                    ))),
                    Location::concrete(token, file),
                ));
            }
        })
    }
}

#[cfg_attr(test, derive(Serialize))]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Visibility {
    FileLocal,
    Package,
    Exposed,
}

impl Visibility {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FileLocal => "file",
            Self::Package => "pkg",
            Self::Exposed => "exposed",
        }
    }
}

impl<'src, 'cxl, 'ctx> Parser<'src, 'cxl, 'ctx> {
    #[recursion_guard]
    pub(super) fn ast(&mut self) -> ParseResult<Option<Ticket<'ctx, Ast<'ctx>>>> {
        let (mut decorators, mut attributes) = (Vec::with_capacity(5), Vec::with_capacity(5));

        while self.peek().is_ok() {
            if let Some(node) = self.ast_impl(&mut decorators, &mut attributes)? {
                let node = self.context.store(node);
                self.symbol_table.push_ast(self.module_scope, node.clone());

                return Ok(Some(node));
            }
        }

        Ok(None)
    }

    // Returns None when the function should be re-called, usually because an attribute or decorator was parsed
    #[recursion_guard]
    fn ast_impl(
        &mut self,
        decorators: &mut Vec<Locatable<Decorator<'ctx>>>,
        attributes: &mut Vec<Locatable<Attribute>>,
    ) -> ParseResult<Option<Ast<'ctx>>> {
        let peek = self.peek()?;
        match peek.ty() {
            TokenType::AtSign => {
                self.decorator(decorators)?;

                Ok(None)
            }

            TokenType::Exposed | TokenType::Package | TokenType::Const => {
                let token = self.next()?;
                let attr = Attribute::try_from((&token, self.current_file))?;
                attributes.push(Locatable::new(
                    attr,
                    Location::concrete(&token, self.current_file),
                ));

                Ok(None)
            }

            TokenType::Function => {
                let func = self.function(mem::take(decorators), mem::take(attributes))?;

                Ok(Some(func))
            }

            TokenType::Type => {
                let ty = self.type_decl(mem::take(decorators), mem::take(attributes))?;

                Ok(Some(ty))
            }

            TokenType::Extend => {
                let extension = self.extend_block(mem::take(decorators), mem::take(attributes))?;

                Ok(Some(extension))
            }

            TokenType::Enum => {
                let enu = self.enum_decl(mem::take(decorators), mem::take(attributes))?;

                Ok(Some(enu))
            }

            TokenType::Trait => {
                let tra = self.trait_decl(mem::take(decorators), mem::take(attributes))?;

                Ok(Some(tra))
            }

            TokenType::Import => {
                if !attributes.is_empty() {
                    Err(Locatable::new(
                        Error::Syntax(SyntaxError::NoAttributesAllowed("import")),
                        Location::concrete(&self.peek()?, self.current_file),
                    ))
                } else {
                    let import = self.import(mem::take(decorators))?;

                    Ok(Some(import))
                }
            }

            TokenType::Alias => {
                let alias = self.alias(mem::take(decorators), mem::take(attributes))?;
                Ok(Some(alias))
            }

            TokenType::Newline | TokenType::Space => {
                self.next()?;
                Ok(None)
            }

            ty => Err(Locatable::new(
                Error::Syntax(SyntaxError::InvalidTopLevel(ty)),
                Location::concrete(&self.peek()?, self.current_file),
            )),
        }
    }

    #[recursion_guard]
    fn import(&mut self, decorators: Vec<Locatable<Decorator<'ctx>>>) -> ParseResult<Ast<'ctx>> {
        let start_span = self.eat(TokenType::Import, [TokenType::Newline])?.span();

        let file = self.eat(TokenType::String, [TokenType::Newline])?;
        let literal = Literal::try_from((&file, self.current_file))?;
        let file = match literal {
            Literal::String(string) => Locatable::new(
                self.context.intern(&string.to_string()),
                Location::concrete(file.span(), self.current_file),
            ),

            lit => {
                let err = if let Literal::Array(_) = lit {
                    Error::Syntax(SyntaxError::ImportByteStringLiteral)
                } else {
                    Error::Syntax(SyntaxError::ImportStringLiteral)
                };

                return Err(Locatable::new(
                    err,
                    Location::concrete(file.span(), self.current_file),
                ));
            }
        };

        let dest = if self.peek()?.ty() == TokenType::Library {
            self.eat(TokenType::Library, [TokenType::Newline])?;

            ImportDest::NativeLib
        } else if self.peek()?.ty() == TokenType::Package {
            self.eat(TokenType::Package, [TokenType::Newline])?;

            ImportDest::Package
        } else {
            ImportDest::default()
        };

        let exposes = if self.peek()?.ty() == TokenType::Exposing {
            self.eat(TokenType::Exposing, [TokenType::Newline])?;

            if self.peek()?.ty() == TokenType::Star {
                self.eat(TokenType::Star, [TokenType::Newline])?;

                ImportExposure::All
            } else {
                let mut members = Vec::with_capacity(5);
                while self.peek()?.ty() != TokenType::Newline {
                    let (span, member) = {
                        let ident = self.eat(TokenType::Ident, [TokenType::Newline])?;
                        (ident.span(), self.context.intern(ident.source()))
                    };

                    let alias = if self.peek()?.ty() == TokenType::As {
                        self.eat(TokenType::As, [TokenType::Newline])?;
                        let alias = {
                            let ident = self.eat(TokenType::Ident, [TokenType::Newline])?;
                            self.context.intern(ident.source())
                        };

                        Some(alias)
                    } else {
                        None
                    };

                    members.push(Locatable::new(
                        (member, alias),
                        Location::concrete(span, self.current_file),
                    ));

                    // TODO: Helpful error if they terminated it too soon
                    if self.peek()?.ty() == TokenType::Comma {
                        self.eat(TokenType::Comma, [TokenType::Newline])?;
                    } else {
                        break;
                    }
                }

                ImportExposure::Members(members)
            }
        } else {
            let alias = if self.peek()?.ty() == TokenType::As {
                self.eat(TokenType::As, [TokenType::Newline])?;

                let ident = self.eat(TokenType::Ident, [TokenType::Newline])?;
                Locatable::new(
                    self.context.intern(ident.source()),
                    Location::concrete(ident.span(), self.current_file),
                )
            } else {
                // Get the last segment of the path as the alias if none is supplied
                let last_segment = self
                    .context
                    .resolve(*file)
                    .split('.')
                    .last()
                    .ok_or(Locatable::new(
                        Error::Syntax(SyntaxError::MissingImport),
                        Location::concrete(&self.peek()?, self.current_file),
                    ))?
                    .to_string();

                Locatable::new(
                    self.context.intern(&last_segment),
                    Location::concrete(file.span(), self.current_file),
                )
            };

            ImportExposure::None(alias)
        };

        let end_span = self.eat(TokenType::Newline, [])?.span();
        let import = Import {
            file,
            dest,
            exposes,
        };

        // Import statements cannot have decorators, so throw an error if there are any
        if decorators.is_empty() {
            Ok(Ast::Import(Locatable::new(
                import,
                Location::concrete(Span::merge(start_span, end_span), self.current_file),
            )))
        } else {
            let first = decorators
                .iter()
                .next()
                .expect("There is at least one decorator")
                .span();

            Err(Locatable::new(
                Error::Syntax(SyntaxError::NoDecoratorsAllowed("import")),
                Location::concrete(
                    Span::merge(
                        first,
                        decorators
                            .iter()
                            .last()
                            .map(Locatable::span)
                            .unwrap_or(first),
                    ),
                    self.current_file,
                ),
            ))
        }
    }

    #[recursion_guard]
    fn trait_decl(
        &mut self,
        decorators: Vec<Locatable<Decorator<'ctx>>>,
        mut attrs: Vec<Locatable<Attribute>>,
    ) -> ParseResult<Ast<'ctx>> {
        let start_span = self.eat(TokenType::Trait, [TokenType::Newline])?.span();
        let name = {
            let ident = self.eat(TokenType::Ident, [TokenType::Newline])?;
            self.context.intern(ident.source())
        };
        let generics = self.generics()?;
        let sig_span_end = self.eat(TokenType::Newline, [])?.span();
        let signature_span = Span::merge(start_span, sig_span_end);

        let (mut method_decorators, mut method_attributes) =
            (Vec::with_capacity(3), Vec::with_capacity(3));

        let mut methods = Vec::with_capacity(4);
        while self.peek()?.ty() != TokenType::End {
            match self.peek()?.ty() {
                TokenType::AtSign => {
                    self.decorator(&mut method_decorators)?;
                }

                TokenType::Exposed | TokenType::Package => {
                    let token = self.next()?;
                    let attr = Attribute::try_from((&token, self.current_file))?;
                    method_attributes.push(Locatable::new(
                        attr,
                        Location::concrete(token, self.current_file),
                    ));
                }

                TokenType::Function => {
                    if !method_attributes.iter().any(|attr| attr.is_visibility()) {
                        method_attributes.push(Locatable::new(
                            Attribute::Visibility(Visibility::FileLocal),
                            Location::implicit(self.current_file.index_span(), self.current_file),
                        ));
                    }

                    let method = self.function(
                        mem::take(&mut method_decorators),
                        mem::take(&mut method_attributes),
                    )?;

                    if let Ast::Function(method) = method {
                        methods.push(method);
                    } else {
                        unreachable!("Something really weird happened")
                    }
                }

                TokenType::Newline => {
                    self.eat(TokenType::Newline, [])?;
                }

                _ => {
                    return Err(Locatable::new(
                        Error::Syntax(SyntaxError::Generic("Only methods, attributes and decorators are allowed inside trait bodies".to_string())),
                        Location::concrete(&self.peek()?, self.current_file),
                    ));
                }
            }
        }
        let end_span = self.eat(TokenType::End, [TokenType::Newline])?.span();

        if !attrs.iter().any(|attr| attr.is_visibility()) {
            attrs.push(Locatable::new(
                Attribute::Visibility(Visibility::FileLocal),
                Location::concrete(signature_span, self.current_file),
            ));
        }

        let trait_decl = Trait {
            decorators,
            attrs,
            name,
            generics,
            methods,
        };

        Ok(Ast::Trait(Locatable::new(
            trait_decl,
            Location::concrete(Span::merge(start_span, end_span), self.current_file),
        )))
    }

    #[recursion_guard]
    fn enum_decl(
        &mut self,
        decorators: Vec<Locatable<Decorator<'ctx>>>,
        mut attrs: Vec<Locatable<Attribute>>,
    ) -> ParseResult<Ast<'ctx>> {
        let start_span = self.eat(TokenType::Enum, [TokenType::Newline])?.span();
        let name = {
            let ident = self.eat(TokenType::Ident, [TokenType::Newline])?;
            self.context.intern(ident.source())
        };
        let generics = self.generics()?;
        let sig_span_end = self.eat(TokenType::Newline, [])?.span();
        let signature_span = Span::merge(start_span, sig_span_end);

        let mut variant_decorators = Vec::with_capacity(7);
        let mut variants = Vec::with_capacity(7);
        while self.peek()?.ty() != TokenType::End {
            match self.peek()?.ty() {
                TokenType::AtSign => {
                    self.decorator(&mut variant_decorators)?;
                }

                TokenType::Ident => {
                    let (name, start_span) = {
                        let ident = self.eat(TokenType::Ident, [TokenType::Newline])?;
                        (self.context.intern(ident.source()), ident.span())
                    };

                    let variant = if self.peek()?.ty() == TokenType::LeftParen {
                        self.eat(TokenType::LeftParen, [TokenType::Newline])?;

                        let mut elements = Vec::with_capacity(3);
                        while self.peek()?.ty() != TokenType::RightParen {
                            let ty = self.ascribed_type()?;
                            elements.push(ty);

                            // TODO: Nice error here
                            if self.peek()?.ty() == TokenType::Comma {
                                self.eat(TokenType::Comma, [TokenType::Newline])?;
                            } else {
                                break;
                            }
                        }
                        self.eat(TokenType::RightParen, [TokenType::Newline])?;

                        EnumVariant::Tuple {
                            name,
                            elements,
                            decorators: mem::take(&mut variant_decorators),
                        }
                    } else {
                        EnumVariant::Unit {
                            name,
                            decorators: mem::take(&mut variant_decorators),
                        }
                    };

                    let end_span = self.eat(TokenType::Newline, [])?.span();

                    variants.push(Locatable::new(
                        variant,
                        Location::concrete(Span::merge(start_span, end_span), self.current_file),
                    ));
                }

                TokenType::Newline => {
                    self.eat(TokenType::Newline, [])?;
                }

                ty => {
                    return Err(Locatable::new(
                        Error::Syntax(SyntaxError::Generic(format!(
                            "Only decorators and enum variants are allowed inside enum declarations, got a `{}`", 
                            ty,
                        ))),
                        Location::concrete(&self.peek()?, self.current_file),
                    ));
                }
            }
        }
        let end_span = self.eat(TokenType::End, [TokenType::Newline])?.span();

        if !attrs.iter().any(|attr| attr.is_visibility()) {
            attrs.push(Locatable::new(
                Attribute::Visibility(Visibility::FileLocal),
                Location::concrete(signature_span, self.current_file),
            ));
        }

        let enum_decl = Enum {
            decorators,
            attrs,
            name,
            generics,
            variants,
        };

        Ok(Ast::Enum(Locatable::new(
            enum_decl,
            Location::concrete(Span::merge(start_span, end_span), self.current_file),
        )))
    }

    #[recursion_guard]
    fn decorator(&mut self, decorators: &mut Vec<Locatable<Decorator<'ctx>>>) -> ParseResult<()> {
        let start = self.eat(TokenType::AtSign, [TokenType::Newline])?.span();
        let (name, name_span) = {
            let ident = self.eat(TokenType::Ident, [TokenType::Newline])?;
            let name = Locatable::new(
                self.context.intern(ident.source()),
                Location::concrete(ident.span(), self.current_file),
            );

            (name, ident.span())
        };

        let (args, end_span) = if self.peek()?.ty() == TokenType::LeftParen {
            self.eat(TokenType::LeftParen, [TokenType::Newline])?;

            let mut args = Vec::with_capacity(5);
            while self.peek()?.ty() != TokenType::RightParen {
                let expr = self.expr()?;
                args.push(expr);

                if let Ok(peek) = self.peek() {
                    if peek.ty() == TokenType::Comma {
                        self.eat(TokenType::Comma, [TokenType::Newline])?;
                        continue;
                    }
                }

                break;
            }
            let end = self
                .eat(TokenType::RightParen, [TokenType::Newline])?
                .span();

            (args, Some(end))
        } else {
            (Vec::new(), None)
        };

        decorators.push(Locatable::new(
            Decorator { name, args },
            Location::concrete(
                Span::merge(start, end_span.unwrap_or(name_span)),
                self.current_file,
            ),
        ));

        Ok(())
    }

    /// ```ebnf
    /// TypeDecl ::=
    ///     Decorator* Attribute* 'type' Ident Generics? '\n'
    ///         (Decorator* Attribute* Ident (':' Type)? '\n')+ | 'empty'
    ///     'end'
    /// ```
    #[recursion_guard]
    fn type_decl(
        &mut self,
        decorators: Vec<Locatable<Decorator<'ctx>>>,
        mut attrs: Vec<Locatable<Attribute>>,
    ) -> ParseResult<Ast<'ctx>> {
        let start_span = self.eat(TokenType::Type, [TokenType::Newline])?.span();
        let name = {
            let ident = self.eat(TokenType::Ident, [TokenType::Newline])?;
            self.context.intern(ident.source())
        };
        let generics = self.generics()?;
        let sig_span_end = self.eat(TokenType::Newline, [])?.span();

        let signature_span = Span::merge(start_span, sig_span_end);

        let (mut member_decorators, mut member_attrs) =
            (Vec::with_capacity(3), Vec::with_capacity(3));

        let mut members = Vec::with_capacity(5);

        while self.peek()?.ty() != TokenType::End {
            match self.peek()?.ty() {
                TokenType::AtSign => {
                    self.decorator(&mut member_decorators)?;
                }

                TokenType::Exposed | TokenType::Package => {
                    let token = self.next()?;
                    let attr = Attribute::try_from((&token, self.current_file))?;
                    member_attrs.push(Locatable::new(
                        attr,
                        Location::concrete(&token, self.current_file),
                    ));
                }

                TokenType::Ident => {
                    let (name, name_span) = {
                        let ident = self.eat(TokenType::Ident, [TokenType::Newline])?;
                        (self.context.intern(ident.source()), ident.span())
                    };

                    let ty = if self.peek()?.ty() == TokenType::Colon {
                        self.eat(TokenType::Colon, [TokenType::Newline])?;
                        self.ascribed_type()?
                    } else {
                        Locatable::new(
                            self.context.store(Type::default()),
                            Location::implicit(name_span, self.current_file),
                        )
                    };

                    if !member_attrs.iter().any(|attr| attr.is_visibility()) {
                        member_attrs.push(Locatable::new(
                            Attribute::Visibility(Visibility::FileLocal),
                            Location::implicit(signature_span, self.current_file),
                        ));
                    }

                    let member = TypeMember {
                        decorators: mem::take(&mut member_decorators),
                        attrs: mem::take(&mut member_attrs),
                        name,
                        ty,
                    };
                    let end_span = self.eat(TokenType::Newline, [])?.span();

                    members.push(Locatable::new(
                        member,
                        Location::concrete(Span::merge(name_span, end_span), self.current_file),
                    ));
                }

                TokenType::Newline => {
                    self.eat(TokenType::Newline, [])?;
                }

                ty => {
                    return Err(Locatable::new(
                        Error::Syntax(SyntaxError::InvalidTopLevel(ty)),
                        Location::concrete(&self.peek()?, self.current_file),
                    ));
                }
            }
        }
        let end_span = self.eat(TokenType::End, [TokenType::Newline])?.span();

        if !member_attrs.is_empty() || !member_decorators.is_empty() {
            return Err(Locatable::new(
                Error::Syntax(SyntaxError::Generic("Attributes and functions must be before members or methods in type declarations".to_string())),
                Location::concrete(&self.peek()?, self.current_file),
            ));
        }

        if !attrs.iter().any(|attr| attr.is_visibility()) {
            attrs.push(Locatable::new(
                Attribute::Visibility(Visibility::FileLocal),
                Location::concrete(signature_span, self.current_file),
            ));
        }

        let type_decl = TypeDecl {
            decorators,
            attrs,
            name,
            generics,
            members,
        };

        Ok(Ast::Type(Locatable::new(
            type_decl,
            Location::concrete(Span::merge(start_span, end_span), self.current_file),
        )))
    }

    /// ```ebnf
    /// ExtendBlock ::=
    ///     Decorator* Attribute* 'extend' Type ('with' Type)? '\n'
    ///         TopNode+ | 'empty'
    ///     'end'
    /// ```
    #[recursion_guard]
    fn extend_block(
        &mut self,
        _decorators: Vec<Locatable<Decorator<'ctx>>>,
        mut _attrs: Vec<Locatable<Attribute>>,
    ) -> ParseResult<Ast<'ctx>> {
        let start = self.eat(TokenType::Extend, [TokenType::Newline])?.span();
        let target = self.ascribed_type()?;

        let extender = if self.peek()?.ty() == TokenType::With {
            self.eat(TokenType::With, [TokenType::Newline])?;
            Some(self.ascribed_type()?)
        } else {
            None
        };

        self.eat(TokenType::Newline, [])?;

        let mut nodes = Vec::with_capacity(5);
        let (mut decorators, mut attributes) = (Vec::with_capacity(5), Vec::with_capacity(5));

        while self.peek()?.ty() != TokenType::End {
            if let Some(node) = self.ast_impl(&mut decorators, &mut attributes)? {
                nodes.push(self.context.store(node));
            }
        }

        if !decorators.is_empty() {
            todo!("error")
        }
        if !attributes.is_empty() {
            todo!("error")
        }

        let end = self.eat(TokenType::End, [])?.span();

        let block = ExtendBlock {
            target,
            extender,
            nodes,
        };

        Ok(Ast::ExtendBlock(Locatable::new(
            block,
            Location::concrete(Span::merge(start, end), self.current_file),
        )))
    }

    /// ```ebnf
    /// Decorator* Attribute* 'alias' Type = Type '\n'
    /// ```
    #[recursion_guard]
    fn alias(
        &mut self,
        decorators: Vec<Locatable<Decorator<'ctx>>>,
        attrs: Vec<Locatable<Attribute>>,
    ) -> ParseResult<Ast<'ctx>> {
        let start = self.eat(TokenType::Alias, [TokenType::Newline])?.span();
        let alias = self.ascribed_type()?;
        self.eat(TokenType::Equal, [TokenType::Newline])?;

        let actual = self.ascribed_type()?;
        let end = self.eat(TokenType::Newline, [])?.span();

        let alias = Alias {
            decorators,
            attrs,
            alias,
            actual,
        };

        Ok(Ast::Alias(Locatable::new(
            alias,
            Location::concrete(Span::merge(start, end), self.current_file),
        )))
    }

    /// ```ebnf
    /// Function ::=
    ///     Decorator* Attribute* 'fn' Ident '(' FunctionArgs* ')' ('->' Type)? '\n'
    ///         Statement* | 'empty'
    ///     'end'
    /// ```
    #[recursion_guard]
    fn function(
        &mut self,
        decorators: Vec<Locatable<Decorator<'ctx>>>,
        mut attrs: Vec<Locatable<Attribute>>,
    ) -> ParseResult<Ast<'ctx>> {
        let start_span = self.eat(TokenType::Function, [TokenType::Newline])?.span();
        let name = {
            let ident = self.eat(TokenType::Ident, [TokenType::Newline])?;
            self.context.intern(ident.source())
        };
        let args = self.function_args()?;

        let returns = if self.peek()?.ty() == TokenType::RightArrow {
            self.eat(TokenType::RightArrow, [])?;
            Some(self.ascribed_type()?)
        } else {
            None
        };
        let sig_end_span = self.eat(TokenType::Newline, [])?.span();
        let signature_span = Span::merge(start_span, sig_end_span);

        let returns = returns.unwrap_or_else(|| {
            Locatable::new(
                self.context.store(Type::default()),
                Location::implicit(signature_span, self.current_file),
            )
        });

        while self.peek()?.ty() == TokenType::Newline {
            self.eat(TokenType::Newline, [])?;
        }

        let mut body = Vec::with_capacity(20);
        while self.peek()?.ty() != TokenType::End {
            if let Some(stmt) = self.stmt()? {
                body.push(stmt);
            }
        }

        let end_span = self.eat(TokenType::End, [TokenType::Newline])?.span();

        if !attrs.iter().any(|attr| attr.is_visibility()) {
            attrs.push(Locatable::new(
                Attribute::Visibility(Visibility::FileLocal),
                Location::implicit(signature_span, self.current_file),
            ));
        }

        let func = Function {
            decorators,
            attrs,
            name,
            args,
            returns,
            body,
        };

        Ok(Ast::Function(Locatable::new(
            func,
            Location::concrete(Span::merge(start_span, end_span), self.current_file),
        )))
    }

    /// ```ebnf
    /// FunctionArgs ::= '(' Args? ')'
    /// Args ::= Argument | Argument ',' Args
    /// Argument ::= Ident ':' Type
    /// ```
    #[recursion_guard]
    fn function_args(&mut self) -> ParseResult<Vec<Locatable<FuncArg<'ctx>>>> {
        self.eat(TokenType::LeftParen, [TokenType::Newline])?;

        let mut args = Vec::with_capacity(7);
        while self.peek()?.ty() != TokenType::RightParen {
            let (comptime, name, name_span) =
                match self.eat_of([TokenType::Ident, TokenType::Const], [TokenType::Newline])? {
                    ident if ident.ty() == TokenType::Ident => (
                        false,
                        Locatable::new(
                            self.context.intern(ident.source()),
                            Location::concrete(ident.span(), self.current_file),
                        ),
                        ident.span(),
                    ),

                    token if token.ty() == TokenType::Const => {
                        let ident = self.eat(TokenType::Ident, [TokenType::Newline])?;
                        let name = Locatable::new(
                            self.context.intern(ident.source()),
                            Location::concrete(ident.span(), self.current_file),
                        );

                        (true, name, token.span())
                    }

                    _ => unreachable!(),
                };

            self.eat(TokenType::Colon, [TokenType::Newline])?;
            let ty = self.ascribed_type()?;
            let arg_end = ty.span();

            let arg = FuncArg { name, ty, comptime };
            let arg_span = Span::merge(name_span, arg_end);

            args.push(Locatable::new(
                arg,
                Location::concrete(arg_span, self.current_file),
            ));

            if self.peek()?.ty() == TokenType::Comma {
                self.eat(TokenType::Comma, [TokenType::Newline])?;
            } else {
                break;
            }
        }
        self.eat(TokenType::RightParen, [TokenType::Newline])?;

        Ok(args)
    }

    /// ```ebnf
    /// Generics ::= '[' GenericArgs? ']'
    /// GenericArgs ::= Type | Type ',' GenericArgs
    /// ```
    #[recursion_guard]
    pub(super) fn generics(&mut self) -> ParseResult<Vec<Locatable<Ticket<'ctx, Type<'ctx>>>>> {
        let peek = if let Ok(peek) = self.peek() {
            peek
        } else {
            return Ok(Vec::new());
        };

        if peek.ty() == TokenType::LeftBrace {
            self.eat(TokenType::LeftBrace, [TokenType::Newline])?;

            let mut generics = Vec::with_capacity(5);
            while self.peek()?.ty() != TokenType::RightBrace {
                generics.push(self.ascribed_type()?);

                if self.peek()?.ty() == TokenType::Comma {
                    self.eat(TokenType::Comma, [TokenType::Newline])?;
                } else {
                    // TODO: Check if next is a `>` and if so emit a helpful error
                    break;
                }
            }
            self.eat(TokenType::RightBrace, [TokenType::Newline])?;

            Ok(generics)
        } else {
            Ok(Vec::new())
        }
    }
}
