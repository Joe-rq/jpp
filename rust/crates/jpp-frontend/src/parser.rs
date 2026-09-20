use crate::{
    ast::*,
    diagnostic::Diagnostic,
    lexer::{self, Kind, Token},
};

pub fn parse(source: &str) -> Result<Program, Diagnostic> {
    let mut parser = Parser {
        tokens: lexer::lex(source)?,
        cursor: 0,
    };
    let budget = if parser.eat("budget") {
        let budget = parser.expr(0)?;
        parser.expect(";")?;
        Some(budget)
    } else {
        None
    };
    let body = parser.body(false, 0)?;
    Ok(Program { budget, body })
}

struct Parser {
    tokens: Vec<Token>,
    cursor: usize,
}
impl Parser {
    fn token(&self) -> &Token {
        &self.tokens[self.cursor]
    }
    fn is(&self, text: &str) -> bool {
        matches!(&self.token().kind, Kind::Name(s) | Kind::Symbol(s) if s == text)
    }
    fn eat(&mut self, text: &str) -> bool {
        if self.is(text) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }
    fn error(&self, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(message, self.token().span)
    }
    fn expect(&mut self, text: &str) -> Result<(), Diagnostic> {
        if self.eat(text) {
            Ok(())
        } else {
            Err(self.error(format!("expected '{text}'")))
        }
    }
    fn name(&mut self) -> Result<String, Diagnostic> {
        if let Kind::Name(name) = &self.token().kind {
            if ["let", "fn", "if", "else", "true", "false", "unit"].contains(&name.as_str()) {
                return Err(self.error("expected an identifier, found a keyword"));
            }
            let name = name.clone();
            self.cursor += 1;
            Ok(name)
        } else {
            Err(self.error("expected an identifier"))
        }
    }
    fn end(&self) -> usize {
        self.tokens[self.cursor.saturating_sub(1)].span.end
    }
    fn annotation(&mut self) -> Result<Option<Type>, Diagnostic> {
        if self.eat(":") {
            Ok(Some(self.ty()?))
        } else {
            Ok(None)
        }
    }
    fn ty(&mut self) -> Result<Type, Diagnostic> {
        let name = self.name()?;
        if name == "Fn" && self.eat("(") {
            let mut inputs = Vec::new();
            if !self.eat(")") {
                loop {
                    inputs.push(self.ty()?);
                    if self.eat(")") {
                        break;
                    }
                    self.expect(",")?;
                }
            }
            self.expect("->")?;
            return Ok(Type::Function(inputs, Box::new(self.ty()?)));
        }
        if self.eat("<") {
            let mut args = Vec::new();
            loop {
                args.push(self.ty()?);
                if self.eat(">") {
                    break;
                }
                self.expect(",")?;
            }
            Ok(Type::Applied(name, args))
        } else {
            Ok(Type::Named(name))
        }
    }
    fn block(&mut self) -> Result<Block, Diagnostic> {
        let start = self.token().span.start;
        self.expect("{")?;
        self.body(true, start)
    }
    fn body(&mut self, braced: bool, start: usize) -> Result<Block, Diagnostic> {
        let mut statements = Vec::new();
        let mut result = None;
        while self.token().kind != Kind::End && !(braced && self.is("}")) {
            let at = self.token().span.start;
            if self.eat("let") {
                let name = self.name()?;
                let annotation = self.annotation()?;
                self.expect("=")?;
                let value = self.expr(0)?;
                self.expect(";")?;
                statements.push(Statement::Let {
                    name,
                    annotation,
                    value,
                    span: Span {
                        start: at,
                        end: self.end(),
                    },
                });
            } else if self.is("fn") && matches!(&self.tokens[self.cursor + 1].kind, Kind::Name(_)) {
                self.cursor += 1;
                let name = self.name()?;
                let function = self.function()?;
                self.eat(";");
                statements.push(Statement::Function {
                    name,
                    function,
                    span: Span {
                        start: at,
                        end: self.end(),
                    },
                });
            } else {
                let value = self.expr(0)?;
                if self.eat(";") {
                    statements.push(Statement::Expression(value));
                } else {
                    result = Some(Box::new(value));
                    break;
                }
            }
        }
        if braced {
            self.expect("}")?;
        } else if self.token().kind != Kind::End {
            return Err(self.error("expected ';' between expressions"));
        }
        Ok(Block {
            statements,
            result,
            span: Span {
                start,
                end: self.end(),
            },
        })
    }
    fn function(&mut self) -> Result<Function, Diagnostic> {
        self.expect("(")?;
        let mut parameters = Vec::new();
        if !self.eat(")") {
            loop {
                let start = self.token().span.start;
                let name = self.name()?;
                let annotation = self.annotation()?;
                parameters.push(Parameter {
                    name,
                    annotation,
                    span: Span {
                        start,
                        end: self.end(),
                    },
                });
                if self.eat(")") {
                    break;
                }
                self.expect(",")?;
                if self.eat(")") {
                    break;
                }
            }
        }
        let result_type = if self.eat("->") {
            Some(self.ty()?)
        } else {
            None
        };
        let effects = if self.eat("!") {
            self.expect("{")?;
            let mut effects = Vec::new();
            if !self.eat("}") {
                loop {
                    effects.push(self.name()?);
                    if self.eat("}") {
                        break;
                    }
                    self.expect(",")?;
                }
            }
            Some(effects)
        } else {
            None
        };
        let body = self.block()?;
        Ok(Function {
            parameters,
            result_type,
            effects,
            body,
        })
    }
    fn expr(&mut self, min: u8) -> Result<Expr, Diagnostic> {
        let start = self.token().span.start;
        let mut left = self.prefix()?;
        loop {
            if self.is("(") && min <= 15 {
                self.cursor += 1;
                let mut arguments = Vec::new();
                if !self.eat(")") {
                    loop {
                        arguments.push(self.expr(0)?);
                        if self.eat(")") {
                            break;
                        }
                        self.expect(",")?;
                        if self.eat(")") {
                            break;
                        }
                    }
                }
                left = Expr {
                    kind: ExprKind::Call {
                        function: Box::new(left),
                        arguments,
                    },
                    span: Span {
                        start,
                        end: self.end(),
                    },
                };
                continue;
            }
            if self.is(".") && min <= 15 {
                self.cursor += 1;
                let field = self.name()?;
                left = Expr {
                    kind: ExprKind::Field {
                        value: Box::new(left),
                        field,
                    },
                    span: Span {
                        start,
                        end: self.end(),
                    },
                };
                continue;
            }
            if self.is("[") && min <= 15 {
                self.cursor += 1;
                let index = self.expr(0)?;
                self.expect("]")?;
                left = Expr {
                    kind: ExprKind::Index {
                        value: Box::new(left),
                        index: Box::new(index),
                    },
                    span: Span {
                        start,
                        end: self.end(),
                    },
                };
                continue;
            }
            let Kind::Symbol(op) = &self.token().kind else {
                break;
            };
            let power = match op.as_str() {
                "||" => 1,
                "&&" => 3,
                "==" | "!=" => 5,
                "<" | ">" | "<=" | ">=" => 7,
                "+" | "-" => 9,
                "*" | "/" | "%" => 11,
                _ => break,
            };
            if power < min {
                break;
            }
            let op = op.clone();
            self.cursor += 1;
            let right = self.expr(power + 1)?;
            left = Expr {
                kind: ExprKind::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span: Span {
                    start,
                    end: self.end(),
                },
            };
        }
        Ok(left)
    }
    fn prefix(&mut self) -> Result<Expr, Diagnostic> {
        let token = self.token().clone();
        let start = token.span.start;
        let kind = if self.eat("if") {
            let condition = Box::new(self.expr(0)?);
            let yes = self.block()?;
            self.expect("else")?;
            let no = if self.is("if") {
                let other = self.expr(0)?;
                let span = other.span;
                Block {
                    statements: vec![],
                    result: Some(Box::new(other)),
                    span,
                }
            } else {
                self.block()?
            };
            ExprKind::If { condition, yes, no }
        } else if self.eat("fn") {
            ExprKind::Function(self.function()?)
        } else if self.eat("!") || self.eat("-") {
            let Kind::Symbol(op) = token.kind else {
                unreachable!()
            };
            ExprKind::Unary {
                op,
                value: Box::new(self.expr(13)?),
            }
        } else if self.eat("(") {
            if self.eat(")") {
                ExprKind::Unit
            } else {
                let expr = self.expr(0)?;
                self.expect(")")?;
                return Ok(Expr {
                    kind: expr.kind,
                    span: Span {
                        start,
                        end: self.end(),
                    },
                });
            }
        } else if self.eat("[") {
            let mut items = Vec::new();
            if !self.eat("]") {
                loop {
                    items.push(self.expr(0)?);
                    if self.eat("]") {
                        break;
                    }
                    self.expect(",")?;
                    if self.eat("]") {
                        break;
                    }
                }
            }
            ExprKind::List(items)
        } else if self.is("{") {
            let record = self
                .tokens
                .get(self.cursor + 1)
                .is_some_and(|t| t.kind == Kind::Symbol("}".into()))
                || self
                    .tokens
                    .get(self.cursor + 2)
                    .is_some_and(|t| t.kind == Kind::Symbol(":".into()));
            if record {
                self.cursor += 1;
                let mut fields = Vec::new();
                if !self.eat("}") {
                    loop {
                        let name = match self.token().kind.clone() {
                            Kind::Text(s) => {
                                self.cursor += 1;
                                s
                            }
                            _ => self.name()?,
                        };
                        if fields.iter().any(|(n, _)| n == &name) {
                            return Err(self.error(format!("duplicate record field '{name}'")));
                        }
                        self.expect(":")?;
                        fields.push((name, self.expr(0)?));
                        if self.eat("}") {
                            break;
                        }
                        self.expect(",")?;
                        if self.eat("}") {
                            break;
                        }
                    }
                }
                ExprKind::Record(fields)
            } else {
                ExprKind::Block(self.block()?)
            }
        } else {
            self.cursor += 1;
            match token.kind {
                Kind::Name(name) => match name.as_str() {
                    "true" => ExprKind::Bool(true),
                    "false" => ExprKind::Bool(false),
                    "unit" => ExprKind::Unit,
                    "let" | "else" => {
                        return Err(Diagnostic::new("expected an expression", token.span));
                    }
                    _ => ExprKind::Name(name),
                },
                Kind::Text(text) => ExprKind::Text(text),
                Kind::Number(n) if n.contains('.') => ExprKind::Decimal(
                    n.parse()
                        .map_err(|_| Diagnostic::new("invalid decimal", token.span))?,
                ),
                Kind::Number(n) => ExprKind::Integer(n.parse().map_err(|_| {
                    Diagnostic::new("integer is outside the signed 64-bit range", token.span)
                })?),
                _ => return Err(Diagnostic::new("expected an expression", token.span)),
            }
        };
        Ok(Expr {
            kind,
            span: Span {
                start,
                end: self.end(),
            },
        })
    }
}
