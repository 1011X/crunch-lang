pub use crate::trees::{
    ast::{
        BinaryOp, CompOp, Float, Integer, Literal as AstLiteral, LiteralVal as AstLiteralVal, Rune,
        Text, Type as AstType,
    },
    Attribute, BlockColor, ItemPath, Signedness, Vis,
};
use crate::{
    error::{Locatable, Location, Span},
    strings::{StrInterner, StrT},
    trees::{CallConv, Sided},
};
#[cfg(feature = "no-std")]
use alloc::{
    borrow::ToOwned,
    string::{String, ToString},
    vec::Vec,
};
use core::fmt::Debug;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct TypeId(usize);

impl TypeId {
    pub(crate) const fn new(id: usize) -> Self {
        Self(id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Item<'ctx> {
    Function(Function<'ctx>),
    ExternFunc(ExternFunc),
    Type(TypeDecl),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Function<'ctx> {
    // TODO: Make this one single StrT
    pub name: ItemPath,
    pub vis: Vis,
    pub args: Locatable<Vec<FuncArg>>,
    pub body: Block<&'ctx Stmt<'ctx>>,
    pub ret: TypeId,
    pub loc: Location,
    pub sig: Location,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct FuncArg {
    pub name: Var,
    pub kind: TypeId,
    pub loc: Location,
}

impl FuncArg {
    pub const fn location(&self) -> Location {
        self.loc
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExternFunc {
    // TODO: Make this one single StrT
    pub name: ItemPath,
    pub vis: Vis,
    pub args: Locatable<Vec<FuncArg>>,
    pub ret: TypeId,
    pub callconv: CallConv,
    pub loc: Location,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeDecl {
    pub generics: Option<Vec<TypeId>>,
    pub members: Vec<TypeMember>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeMember {
    pub name: StrT,
    pub ty: TypeId,
    pub attrs: Vec<Attribute>,
    pub loc: Location,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Stmt<'ctx> {
    Item(&'ctx Item<'ctx>),
    Expr(&'ctx Expr<'ctx>),
    // TODO: Maybe arena these
    VarDecl(VarDecl<'ctx>),
}

impl<'ctx> From<&'ctx Item<'ctx>> for Stmt<'ctx> {
    fn from(item: &'ctx Item<'ctx>) -> Self {
        Self::Item(item)
    }
}

impl<'ctx> From<&'ctx Expr<'ctx>> for Stmt<'ctx> {
    fn from(expr: &'ctx Expr<'ctx>) -> Self {
        Self::Expr(expr)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Expr<'ctx> {
    pub kind: ExprKind<'ctx>,
    pub loc: Location,
}

impl<'ctx> Expr<'ctx> {
    pub const fn location(&self) -> Location {
        self.loc
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ExprKind<'ctx> {
    Match(Match<'ctx>),
    Scope(Block<&'ctx Stmt<'ctx>>),
    Loop(Block<&'ctx Stmt<'ctx>>),
    Return(Return<'ctx>),
    Continue,
    Break(Break<'ctx>),
    FnCall(FuncCall<'ctx>),
    Literal(Literal<'ctx>),
    Comparison(Sided<CompOp, &'ctx Expr<'ctx>>),
    Variable(Var, TypeId),
    Assign(Var, &'ctx Expr<'ctx>),
    BinOp(Sided<BinaryOp, &'ctx Expr<'ctx>>),
    Cast(Cast<'ctx>),
    Reference(Reference<'ctx>),
    Index { var: Var, index: &'ctx Expr<'ctx> },
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Var {
    User(StrT),
    // TODO: Make this a u32 so they're the same size
    Auto(usize),
}

impl Var {
    pub fn to_string(&self, interner: &StrInterner) -> String {
        match *self {
            Self::User(var) => interner.resolve(var).as_ref().to_owned(),
            Self::Auto(var) => var.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VarDecl<'ctx> {
    pub name: Var,
    pub value: &'ctx Expr<'ctx>,
    pub mutable: bool,
    pub ty: TypeId,
    pub loc: Location,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FuncCall<'ctx> {
    pub func: ItemPath,
    pub args: Vec<&'ctx Expr<'ctx>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Match<'ctx> {
    pub cond: &'ctx Expr<'ctx>,
    // TODO: Arena match arms
    pub arms: Vec<MatchArm<'ctx>>,
    pub ty: TypeId,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MatchArm<'ctx> {
    // TODO: Arena & dedup bindings
    pub bind: Binding<'ctx>,
    pub guard: Option<&'ctx Expr<'ctx>>,
    pub body: Block<&'ctx Stmt<'ctx>>,
    pub ty: TypeId,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Binding<'ctx> {
    // TODO: Enum for mutability/referential status?
    pub reference: bool,
    pub mutable: bool,
    pub pattern: Pattern<'ctx>,
    pub ty: Option<TypeId>,
}

// TODO: Arena & dedup patterns
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Pattern<'ctx> {
    Literal(Literal<'ctx>),
    Ident(StrT),
    ItemPath(ItemPath),
    Wildcard,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Return<'ctx> {
    pub val: Option<&'ctx Expr<'ctx>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Break<'ctx> {
    pub val: Option<&'ctx Expr<'ctx>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Block<T> {
    pub block: Vec<T>,
    pub colors: Vec<BlockColor>,
    pub loc: Location,
}

impl<T> Block<T> {
    pub fn new(block: Vec<T>, loc: Location) -> Self {
        Self {
            block,
            colors: Vec::new(),
            loc,
        }
    }

    pub fn empty(loc: Location) -> Self {
        Self {
            block: Vec::new(),
            colors: Vec::new(),
            loc,
        }
    }

    pub fn with_capacity(loc: Location, capacity: usize) -> Self {
        Self {
            block: Vec::with_capacity(capacity),
            colors: Vec::new(),
            loc,
        }
    }

    pub fn with_capacity_and_colors(loc: Location, capacity: usize, colors: usize) -> Self {
        Self {
            block: Vec::with_capacity(capacity),
            colors: Vec::with_capacity(colors),
            loc,
        }
    }

    pub fn push(&mut self, item: T) {
        self.block.push(item);
    }

    pub fn push_color(&mut self, color: BlockColor) {
        self.colors.push(color);
    }

    pub fn extend_colors<I>(&mut self, colors: I)
    where
        I: Iterator<Item = BlockColor>,
    {
        self.colors.extend(colors);
    }

    pub fn insert(&mut self, idx: usize, item: T) {
        self.block.insert(idx, item);
    }

    pub fn location(&self) -> Location {
        self.loc
    }

    pub fn span(&self) -> Span {
        self.loc.span()
    }

    pub fn len(&self) -> usize {
        self.block.len()
    }

    pub fn is_empty(&self) -> bool {
        self.block.is_empty()
    }

    pub fn iter<'a>(
        &'a self,
    ) -> impl Iterator<Item = &'a T> + ExactSizeIterator + DoubleEndedIterator + 'a {
        self.block.iter()
    }

    pub fn iter_mut<'a>(
        &'a mut self,
    ) -> impl Iterator<Item = &'a mut T> + ExactSizeIterator + DoubleEndedIterator + 'a {
        self.block.iter_mut()
    }

    pub fn from_iter<I: IntoIterator<Item = T>>(loc: Location, iter: I) -> Self {
        let mut block = Vec::with_capacity(10);
        for item in iter {
            block.push(item);
        }

        Self {
            block,
            colors: Vec::new(),
            loc,
        }
    }
}

impl<T> Block<T>
where
    T: Clone,
{
    pub fn extend_from_slice<S>(&mut self, slice: S)
    where
        S: AsRef<[T]>,
    {
        self.block.extend_from_slice(slice.as_ref())
    }
}

impl<T> Extend<T> for Block<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        self.block.extend(iter)
    }
}

/// A type
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Type {
    /// The kind of type this type is
    pub kind: TypeKind,
    /// The type's source location
    pub loc: Location,
}

impl Type {
    /// Creates a new `Type`
    pub const fn new(kind: TypeKind, loc: Location) -> Self {
        Self { kind, loc }
    }

    /// Returns the type's location
    pub const fn location(&self) -> Location {
        self.loc
    }

    /// Returns `true` if the type is `Unit`
    pub fn is_unit(&self) -> bool {
        self.kind.is_unit()
    }

    /// Returns `true` if the type is `Unknown`
    pub fn is_unknown(&self) -> bool {
        self.kind.is_unknown()
    }

    pub fn is_array(&self) -> bool {
        self.kind.is_array()
    }

    pub fn is_slice(&self) -> bool {
        self.kind.is_array()
    }
}

/// The type that a type actually is
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum TypeKind {
    /// An unknown type
    Unknown,
    /// An integer of potentially unknown width & sign
    Integer {
        /// Whether the integer is signed or not, `None` for unknown sign
        signed: Option<bool>,
        /// The integer's width, `None` for an unknown width
        width: Option<u16>,
    },
    /// A string
    String,
    /// A boolean
    Bool,
    /// The unit type
    Unit,
    /// The absurd type
    Absurd,
    /// An array type, arr[_; _]
    Array {
        /// The type of the array's elements
        element: TypeId,
        /// The length of the array
        length: u64,
    },
    /// A slice type, slice[_]
    Slice {
        /// The type of the slice's elements
        element: TypeId,
    },
    /// A reference type, &_ or &mut _
    Reference {
        /// The type the reference points to
        referee: TypeId,
        /// Whether the reference is mutable or not
        mutable: bool,
    },
    /// A pointer type, *const _ or *mut _
    Pointer {
        /// The type the pointer points to
        pointee: TypeId,
        /// Whether the pointer is mutable or not
        mutable: bool,
    },
    /// A type with the type of another type
    Variable(TypeId),
}

impl TypeKind {
    /// Returns `true` if the type is `Unit`
    pub fn is_unit(&self) -> bool {
        matches!(self, Self::Unit)
    }

    /// Returns `true` if the type is `Unknown`
    pub fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown)
    }

    pub fn is_array(&self) -> bool {
        matches!(self, Self::Array { .. })
    }

    pub fn is_slice(&self) -> bool {
        matches!(self, Self::Slice { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Cast<'ctx> {
    pub casted: &'ctx Expr<'ctx>,
    pub ty: TypeId,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Reference<'ctx> {
    pub mutable: bool,
    pub reference: &'ctx Expr<'ctx>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Literal<'ctx> {
    pub val: LiteralVal<'ctx>,
    pub ty: TypeId,
    pub loc: Location,
}

impl<'ctx> Literal<'ctx> {
    pub const fn location(&self) -> Location {
        self.loc
    }
}

// TODO: Arena & dedup literals
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LiteralVal<'ctx> {
    Integer(Integer),
    Bool(bool),
    String(Text),
    Rune(Rune),
    Float(Float),
    Array { elements: Vec<Literal<'ctx>> },
    Struct(StructLiteral<'ctx>),
    // TODO: Tuples, slices, records, others?
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StructLiteral<'ctx> {
    pub name: StrT,
    pub fields: Vec<StructField<'ctx>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StructField<'ctx> {
    pub name: StrT,
    pub value: &'ctx Expr<'ctx>,
    pub loc: Location,
}
