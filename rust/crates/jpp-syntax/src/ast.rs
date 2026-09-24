//! Source syntax only. The shared core owns executable programs and values.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Program {
    pub budget: Option<Expr>,
    pub body: Block,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub statements: Vec<Statement>,
    pub result: Option<Box<Expr>>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Statement {
    Let {
        name: String,
        annotation: Option<Type>,
        value: Expr,
        span: Span,
    },
    Function {
        name: String,
        function: Function,
        span: Span,
    },
    Expression(Expr),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind {
    Integer(i64),
    Decimal(f64),
    Bool(bool),
    Text(String),
    Unit,
    Name(String),
    List(Vec<Expr>),
    Record(Vec<(String, Expr)>),
    Function(Function),
    Call {
        function: Box<Expr>,
        arguments: Vec<Expr>,
    },
    Field {
        value: Box<Expr>,
        field: String,
    },
    Index {
        value: Box<Expr>,
        index: Box<Expr>,
    },
    Unary {
        op: String,
        value: Box<Expr>,
    },
    Binary {
        op: String,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    If {
        condition: Box<Expr>,
        yes: Block,
        no: Block,
    },
    Block(Block),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Function {
    pub parameters: Vec<Parameter>,
    pub result_type: Option<Type>,
    /// None means unspecified; Some([]) is an explicit pure declaration.
    pub effects: Option<Vec<String>>,
    pub body: Block,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Parameter {
    pub name: String,
    pub annotation: Option<Type>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Type {
    Named(String),
    Applied(String, Vec<Type>),
    Function(Vec<Type>, Box<Type>),
    Method {
        parameters: Vec<Type>,
        result: Box<Type>,
        effects: Option<Vec<String>>,
        captures_responsibility: bool,
    },
}
