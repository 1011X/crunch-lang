use super::{CodeBuilder, FunctionContext};

use crunch_error::compile_prelude::*;
use crunch_parser::{
    ast::{Exposure, Type},
    string_interner::Sym,
};

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct TypeContext {
    name: Sym,
    members: HashMap<Sym, (Exposure, Type)>,
    methods: HashMap<Sym, (Exposure, FunctionContext)>,
}

impl TypeContext {
    #[inline]
    pub fn new(name: Sym) -> Self {
        Self {
            name,
            members: HashMap::new(),
            methods: HashMap::new(),
        }
    }

    pub fn add_member(&mut self, name: Sym, visibility: Exposure, ty: Type) -> &mut Self {
        self.members.insert(name, (visibility, ty));

        self
    }

    pub fn add_method<M>(
        &mut self,
        builder: &mut CodeBuilder,
        name: Sym,
        visibility: Exposure,
        method: M,
    ) -> CompileResult<&mut Self>
    where
        M: FnOnce(&mut CodeBuilder, &mut FunctionContext) -> CompileResult<()>,
    {
        let mut context = FunctionContext::new(name);

        (method)(builder, &mut context)?;

        self.methods.insert(name, (visibility, context));

        Ok(self)
    }
}