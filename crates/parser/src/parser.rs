//! Recursive-descent / precedence-climbing parser that drives
//! `ssa_builder::SsaBuilder` directly: every expression and statement is
//! turned into typed SSA values and control flow the moment it's
//! recognized, with no `Expr`/`Stmt` AST node ever constructed in
//! between. Type checking is folded in the same way — a binary op's
//! operand types are checked the instant both operands are already
//! typed `Value`s.
//!
//! Two passes over the token stream, both driven by the same `Parser`:
//! `scan_signatures` records every function's name/params/return type
//! (skipping bodies via brace matching) so forward and mutually
//! recursive calls resolve correctly, then a second pass parses each
//! function's body with the complete signature table already available.

use std::collections::HashMap;

use ir::{BinOp, InstKind, Module, Terminator, Type, UnOp};
use lexer::{lex, LexError, Span, Token, TokenKind};

use crate::ssa_builder::SsaBuilder;

/// A parsed function header: name, `(param name, type)` pairs, and
/// return type.
type FnHeader = (String, Vec<(String, Type)>, Option<Type>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnSig {
    pub params: Vec<Type>,
    pub ret_ty: Option<Type>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ParseError {
    #[error(transparent)]
    Lex(#[from] LexError),
    #[error("expected {expected}, found {found} at byte offset {}", span.start)]
    UnexpectedToken {
        expected: String,
        found: TokenKind,
        span: Span,
    },
    #[error("duplicate function '{name}'")]
    DuplicateFunction { name: String },
    #[error("duplicate parameter '{name}' in function '{func}'")]
    DuplicateParameter { func: String, name: String },
    #[error("undefined variable '{name}' at byte offset {}", span.start)]
    UndefinedVariable { name: String, span: Span },
    #[error("call to undefined function '{name}' at byte offset {}", span.start)]
    UndefinedFunction { name: String, span: Span },
    #[error("'{func}' expects {expected} argument(s), got {given} at byte offset {}", span.start)]
    ArgCountMismatch {
        func: String,
        expected: usize,
        given: usize,
        span: Span,
    },
    #[error("type mismatch: expected {expected}, got {actual} at byte offset {}", span.start)]
    TypeMismatch {
        expected: Type,
        actual: Type,
        span: Span,
    },
    #[error("unreachable code after a terminating statement, at byte offset {}", span.start)]
    UnreachableCode { span: Span },
}

/// Lexes and parses `source` into a validated `ir::Module`.
pub fn parse(source: &str) -> Result<Module, ParseError> {
    let tokens = lex(source)?;
    let mut parser = Parser::new(tokens);
    parser.parse_module()
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    sigs: HashMap<String, FnSig>,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            sigs: HashMap::new(),
        }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.peek().kind
    }

    fn advance(&mut self) -> Token {
        let t = self.tokens[self.pos].clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        t
    }

    fn expect(&mut self, kind: &TokenKind) -> Result<Token, ParseError> {
        if self.peek_kind() == kind {
            Ok(self.advance())
        } else {
            Err(ParseError::UnexpectedToken {
                expected: kind.to_string(),
                found: self.peek_kind().clone(),
                span: self.peek().span,
            })
        }
    }

    fn expect_ident(&mut self) -> Result<String, ParseError> {
        match self.peek_kind().clone() {
            TokenKind::Ident(name) => {
                self.advance();
                Ok(name)
            }
            other => Err(ParseError::UnexpectedToken {
                expected: "an identifier".to_string(),
                found: other,
                span: self.peek().span,
            }),
        }
    }

    fn expect_type(&mut self) -> Result<Type, ParseError> {
        match self.peek_kind() {
            TokenKind::I64 => {
                self.advance();
                Ok(Type::I64)
            }
            TokenKind::Bool => {
                self.advance();
                Ok(Type::Bool)
            }
            other => Err(ParseError::UnexpectedToken {
                expected: "a type ('i64' or 'bool')".to_string(),
                found: other.clone(),
                span: self.peek().span,
            }),
        }
    }

    // ---- Pass 1: signatures ----

    fn parse_module(&mut self) -> Result<Module, ParseError> {
        let mut fn_starts = Vec::new();
        while *self.peek_kind() != TokenKind::Eof {
            fn_starts.push(self.pos);
            let (name, params, ret_ty) = self.parse_fn_header()?;
            if self
                .sigs
                .insert(
                    name.clone(),
                    FnSig {
                        params: params.iter().map(|(_, t)| *t).collect(),
                        ret_ty,
                    },
                )
                .is_some()
            {
                return Err(ParseError::DuplicateFunction { name });
            }
            self.skip_block()?;
        }

        self.pos = 0;
        let mut functions = Vec::with_capacity(fn_starts.len());
        for start in fn_starts {
            self.pos = start;
            functions.push(self.parse_function()?);
        }

        Ok(Module { functions })
    }

    /// Parses `fn name(params) -> type`, leaving the cursor at the
    /// following `{`. Shared by both passes so the header is only ever
    /// described in one place.
    fn parse_fn_header(&mut self) -> Result<FnHeader, ParseError> {
        self.expect(&TokenKind::Fn)?;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LParen)?;
        let mut params = Vec::new();
        if *self.peek_kind() != TokenKind::RParen {
            loop {
                let pname = self.expect_ident()?;
                self.expect(&TokenKind::Colon)?;
                let pty = self.expect_type()?;
                if params.iter().any(|(n, _): &(String, Type)| n == &pname) {
                    return Err(ParseError::DuplicateParameter {
                        func: name.clone(),
                        name: pname,
                    });
                }
                params.push((pname, pty));
                if *self.peek_kind() == TokenKind::Comma {
                    self.advance();
                } else {
                    break;
                }
            }
        }
        self.expect(&TokenKind::RParen)?;
        self.expect(&TokenKind::Arrow)?;
        let ret_ty = Some(self.expect_type()?);
        Ok((name, params, ret_ty))
    }

    /// Skips a `{ ... }` block via brace matching, without interpreting
    /// its contents — used by pass 1 to jump straight to the next
    /// function header.
    fn skip_block(&mut self) -> Result<(), ParseError> {
        self.expect(&TokenKind::LBrace)?;
        let mut depth = 1i32;
        while depth > 0 {
            match self.peek_kind() {
                TokenKind::LBrace => depth += 1,
                TokenKind::RBrace => depth -= 1,
                TokenKind::Eof => {
                    return Err(ParseError::UnexpectedToken {
                        expected: "'}'".to_string(),
                        found: TokenKind::Eof,
                        span: self.peek().span,
                    })
                }
                _ => {}
            }
            self.advance();
        }
        Ok(())
    }

    // ---- Pass 2: bodies ----

    fn parse_function(&mut self) -> Result<ir::Function, ParseError> {
        let (name, params, ret_ty) = self.parse_fn_header()?;
        let mut ctx = FnContext {
            builder: SsaBuilder::new(name, params.clone(), ret_ty),
            scopes: vec![params.into_iter().collect()],
            current_block: ir::BlockId(0),
            ret_ty,
            sigs: self.sigs.clone(),
        };
        ctx.current_block = ctx.builder.entry();
        self.parse_block_body(&mut ctx)?;
        Ok(ctx.builder.finish())
    }

    fn parse_block_body(&mut self, ctx: &mut FnContext) -> Result<(), ParseError> {
        self.expect(&TokenKind::LBrace)?;
        ctx.scopes.push(HashMap::new());
        while *self.peek_kind() != TokenKind::RBrace {
            if ctx.builder.is_terminated(ctx.current_block) {
                return Err(ParseError::UnreachableCode {
                    span: self.peek().span,
                });
            }
            self.parse_statement(ctx)?;
        }
        self.expect(&TokenKind::RBrace)?;
        ctx.scopes.pop();
        Ok(())
    }

    fn parse_statement(&mut self, ctx: &mut FnContext) -> Result<(), ParseError> {
        match self.peek_kind() {
            TokenKind::Let => self.parse_let(ctx),
            TokenKind::If => self.parse_if(ctx),
            TokenKind::While => self.parse_while(ctx),
            TokenKind::Return => self.parse_return(ctx),
            TokenKind::Ident(_) if self.peek_is_assignment() => self.parse_assign(ctx),
            _ => {
                self.parse_expr(ctx)?;
                self.expect(&TokenKind::Semicolon)?;
                Ok(())
            }
        }
    }

    /// True if the upcoming tokens are `Ident '='` (an assignment), as
    /// opposed to `Ident` starting a call or bare-variable expression
    /// statement. One token of lookahead beyond the identifier.
    fn peek_is_assignment(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Ident(_))
            && self.tokens.get(self.pos + 1).map(|t| &t.kind) == Some(&TokenKind::Eq)
    }

    fn parse_let(&mut self, ctx: &mut FnContext) -> Result<(), ParseError> {
        self.expect(&TokenKind::Let)?;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::Eq)?;
        let (value, ty) = self.parse_expr(ctx)?;
        self.expect(&TokenKind::Semicolon)?;
        ctx.scopes.last_mut().unwrap().insert(name.clone(), ty);
        ctx.builder.write_variable(&name, ctx.current_block, value);
        Ok(())
    }

    fn parse_assign(&mut self, ctx: &mut FnContext) -> Result<(), ParseError> {
        let name_span = self.peek().span;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::Eq)?;
        let (value, actual) = self.parse_expr(ctx)?;
        self.expect(&TokenKind::Semicolon)?;
        let expected = ctx
            .lookup_var(&name)
            .ok_or_else(|| ParseError::UndefinedVariable {
                name: name.clone(),
                span: name_span,
            })?;
        if expected != actual {
            return Err(ParseError::TypeMismatch {
                expected,
                actual,
                span: name_span,
            });
        }
        ctx.builder.write_variable(&name, ctx.current_block, value);
        Ok(())
    }

    fn parse_return(&mut self, ctx: &mut FnContext) -> Result<(), ParseError> {
        let span = self.peek().span;
        self.expect(&TokenKind::Return)?;
        match ctx.ret_ty {
            Some(expected) => {
                let (value, actual) = self.parse_expr(ctx)?;
                if actual != expected {
                    return Err(ParseError::TypeMismatch {
                        expected,
                        actual,
                        span,
                    });
                }
                self.expect(&TokenKind::Semicolon)?;
                ctx.builder
                    .terminate(ctx.current_block, Terminator::Return(Some(value)));
            }
            None => {
                self.expect(&TokenKind::Semicolon)?;
                ctx.builder
                    .terminate(ctx.current_block, Terminator::Return(None));
            }
        }
        Ok(())
    }

    fn parse_if(&mut self, ctx: &mut FnContext) -> Result<(), ParseError> {
        let span = self.peek().span;
        self.expect(&TokenKind::If)?;
        let (cond, cond_ty) = self.parse_expr(ctx)?;
        if cond_ty != Type::Bool {
            return Err(ParseError::TypeMismatch {
                expected: Type::Bool,
                actual: cond_ty,
                span,
            });
        }

        let then_block = ctx.builder.new_block();
        let else_block = ctx.builder.new_block();
        ctx.builder.terminate(
            ctx.current_block,
            Terminator::Branch {
                cond,
                then_block,
                else_block,
            },
        );
        ctx.builder.seal_block(then_block);
        ctx.builder.seal_block(else_block);

        ctx.current_block = then_block;
        self.parse_block_body(ctx)?;
        let then_tail = ctx.current_block;
        let then_terminated = ctx.builder.is_terminated(then_tail);

        ctx.current_block = else_block;
        let else_terminated = if *self.peek_kind() == TokenKind::Else {
            self.advance();
            if *self.peek_kind() == TokenKind::If {
                self.parse_if(ctx)?;
            } else {
                self.parse_block_body(ctx)?;
            }
            ctx.builder.is_terminated(ctx.current_block)
        } else {
            false
        };
        let else_tail = ctx.current_block;

        if then_terminated && else_terminated {
            // Both arms already end in their own terminator (e.g. both
            // `return`) — a join block would have zero predecessors and
            // nothing to ever fill it in, which is exactly the "empty
            // block, no terminator" shape validation rejects. Leave
            // `current_block` pointing at an already-terminated tail so
            // the enclosing `parse_block_body`'s is_terminated check
            // correctly treats anything after this `if` as unreachable,
            // instead of creating a join block no one will ever finish.
            ctx.current_block = then_tail;
        } else {
            let join = ctx.builder.new_block();
            if !then_terminated {
                ctx.builder.terminate(then_tail, Terminator::Jump(join));
            }
            if !else_terminated {
                ctx.builder.terminate(else_tail, Terminator::Jump(join));
            }
            ctx.builder.seal_block(join);
            ctx.current_block = join;
        }
        Ok(())
    }

    fn parse_while(&mut self, ctx: &mut FnContext) -> Result<(), ParseError> {
        let header = ctx.builder.new_block();
        ctx.builder
            .terminate(ctx.current_block, Terminator::Jump(header));
        ctx.current_block = header;

        self.expect(&TokenKind::While)?;
        let span = self.peek().span;
        let (cond, cond_ty) = self.parse_expr(ctx)?;
        if cond_ty != Type::Bool {
            return Err(ParseError::TypeMismatch {
                expected: Type::Bool,
                actual: cond_ty,
                span,
            });
        }

        let body = ctx.builder.new_block();
        let exit = ctx.builder.new_block();
        ctx.builder.terminate(
            header,
            Terminator::Branch {
                cond,
                then_block: body,
                else_block: exit,
            },
        );
        ctx.builder.seal_block(body);
        ctx.builder.seal_block(exit);

        ctx.current_block = body;
        self.parse_block_body(ctx)?;
        let body_tail = ctx.current_block;
        if !ctx.builder.is_terminated(body_tail) {
            ctx.builder.terminate(body_tail, Terminator::Jump(header));
        }
        ctx.builder.seal_block(header);

        ctx.current_block = exit;
        Ok(())
    }

    // ---- Expressions ----

    fn parse_expr(&mut self, ctx: &mut FnContext) -> Result<(ir::ValueId, Type), ParseError> {
        self.parse_or(ctx)
    }

    fn parse_or(&mut self, ctx: &mut FnContext) -> Result<(ir::ValueId, Type), ParseError> {
        let (mut lhs, mut lty) = self.parse_and(ctx)?;
        while *self.peek_kind() == TokenKind::Or {
            let span = self.peek().span;
            self.advance();
            if lty != Type::Bool {
                return Err(ParseError::TypeMismatch {
                    expected: Type::Bool,
                    actual: lty,
                    span,
                });
            }
            let short_block = ctx.builder.new_block();
            let rhs_block = ctx.builder.new_block();
            let join = ctx.builder.new_block();
            ctx.builder.terminate(
                ctx.current_block,
                Terminator::Branch {
                    cond: lhs,
                    then_block: short_block,
                    else_block: rhs_block,
                },
            );
            ctx.builder.seal_block(short_block);
            ctx.builder.seal_block(rhs_block);

            let true_val = ctx
                .builder
                .emit(short_block, Type::Bool, InstKind::ConstBool(true));
            ctx.builder.terminate(short_block, Terminator::Jump(join));

            ctx.current_block = rhs_block;
            let (rval, rty) = self.parse_and(ctx)?;
            if rty != Type::Bool {
                return Err(ParseError::TypeMismatch {
                    expected: Type::Bool,
                    actual: rty,
                    span,
                });
            }
            let rhs_tail = ctx.current_block;
            ctx.builder.terminate(rhs_tail, Terminator::Jump(join));
            ctx.builder.seal_block(join);

            lhs = ctx.builder.emit_phi(
                join,
                Type::Bool,
                vec![(short_block, true_val), (rhs_tail, rval)],
            );
            lty = Type::Bool;
            ctx.current_block = join;
        }
        Ok((lhs, lty))
    }

    fn parse_and(&mut self, ctx: &mut FnContext) -> Result<(ir::ValueId, Type), ParseError> {
        let (mut lhs, mut lty) = self.parse_not(ctx)?;
        while *self.peek_kind() == TokenKind::And {
            let span = self.peek().span;
            self.advance();
            if lty != Type::Bool {
                return Err(ParseError::TypeMismatch {
                    expected: Type::Bool,
                    actual: lty,
                    span,
                });
            }
            let short_block = ctx.builder.new_block();
            let rhs_block = ctx.builder.new_block();
            let join = ctx.builder.new_block();
            ctx.builder.terminate(
                ctx.current_block,
                Terminator::Branch {
                    cond: lhs,
                    then_block: rhs_block,
                    else_block: short_block,
                },
            );
            ctx.builder.seal_block(short_block);
            ctx.builder.seal_block(rhs_block);

            let false_val = ctx
                .builder
                .emit(short_block, Type::Bool, InstKind::ConstBool(false));
            ctx.builder.terminate(short_block, Terminator::Jump(join));

            ctx.current_block = rhs_block;
            let (rval, rty) = self.parse_not(ctx)?;
            if rty != Type::Bool {
                return Err(ParseError::TypeMismatch {
                    expected: Type::Bool,
                    actual: rty,
                    span,
                });
            }
            let rhs_tail = ctx.current_block;
            ctx.builder.terminate(rhs_tail, Terminator::Jump(join));
            ctx.builder.seal_block(join);

            lhs = ctx.builder.emit_phi(
                join,
                Type::Bool,
                vec![(short_block, false_val), (rhs_tail, rval)],
            );
            lty = Type::Bool;
            ctx.current_block = join;
        }
        Ok((lhs, lty))
    }

    fn parse_not(&mut self, ctx: &mut FnContext) -> Result<(ir::ValueId, Type), ParseError> {
        if *self.peek_kind() == TokenKind::Not {
            let span = self.peek().span;
            self.advance();
            let (v, ty) = self.parse_not(ctx)?;
            if ty != Type::Bool {
                return Err(ParseError::TypeMismatch {
                    expected: Type::Bool,
                    actual: ty,
                    span,
                });
            }
            let result =
                ctx.builder
                    .emit(ctx.current_block, Type::Bool, InstKind::Un(UnOp::Not, v));
            Ok((result, Type::Bool))
        } else {
            self.parse_cmp(ctx)
        }
    }

    fn parse_cmp(&mut self, ctx: &mut FnContext) -> Result<(ir::ValueId, Type), ParseError> {
        let (lhs, lty) = self.parse_add(ctx)?;
        let op = match self.peek_kind() {
            TokenKind::EqEq => Some(BinOp::CmpEq),
            TokenKind::NotEq => Some(BinOp::CmpNe),
            TokenKind::Lt => Some(BinOp::CmpLt),
            TokenKind::Le => Some(BinOp::CmpLe),
            TokenKind::Gt => Some(BinOp::CmpGt),
            TokenKind::Ge => Some(BinOp::CmpGe),
            _ => None,
        };
        let Some(op) = op else {
            return Ok((lhs, lty));
        };
        let span = self.peek().span;
        self.advance();
        if lty != Type::I64 {
            return Err(ParseError::TypeMismatch {
                expected: Type::I64,
                actual: lty,
                span,
            });
        }
        let (rhs, rty) = self.parse_add(ctx)?;
        if rty != Type::I64 {
            return Err(ParseError::TypeMismatch {
                expected: Type::I64,
                actual: rty,
                span,
            });
        }
        let result = ctx
            .builder
            .emit(ctx.current_block, Type::Bool, InstKind::Bin(op, lhs, rhs));
        Ok((result, Type::Bool))
    }

    fn parse_add(&mut self, ctx: &mut FnContext) -> Result<(ir::ValueId, Type), ParseError> {
        let (mut lhs, mut lty) = self.parse_mul(ctx)?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Plus => Some(BinOp::Add),
                TokenKind::Minus => Some(BinOp::Sub),
                _ => None,
            };
            let Some(op) = op else { break };
            let span = self.peek().span;
            self.advance();
            if lty != Type::I64 {
                return Err(ParseError::TypeMismatch {
                    expected: Type::I64,
                    actual: lty,
                    span,
                });
            }
            let (rhs, rty) = self.parse_mul(ctx)?;
            if rty != Type::I64 {
                return Err(ParseError::TypeMismatch {
                    expected: Type::I64,
                    actual: rty,
                    span,
                });
            }
            lhs = ctx
                .builder
                .emit(ctx.current_block, Type::I64, InstKind::Bin(op, lhs, rhs));
            lty = Type::I64;
        }
        Ok((lhs, lty))
    }

    fn parse_mul(&mut self, ctx: &mut FnContext) -> Result<(ir::ValueId, Type), ParseError> {
        let (mut lhs, mut lty) = self.parse_unary(ctx)?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Star => Some(BinOp::Mul),
                TokenKind::Slash => Some(BinOp::Div),
                TokenKind::Percent => Some(BinOp::Mod),
                _ => None,
            };
            let Some(op) = op else { break };
            let span = self.peek().span;
            self.advance();
            if lty != Type::I64 {
                return Err(ParseError::TypeMismatch {
                    expected: Type::I64,
                    actual: lty,
                    span,
                });
            }
            let (rhs, rty) = self.parse_unary(ctx)?;
            if rty != Type::I64 {
                return Err(ParseError::TypeMismatch {
                    expected: Type::I64,
                    actual: rty,
                    span,
                });
            }
            lhs = ctx
                .builder
                .emit(ctx.current_block, Type::I64, InstKind::Bin(op, lhs, rhs));
            lty = Type::I64;
        }
        Ok((lhs, lty))
    }

    fn parse_unary(&mut self, ctx: &mut FnContext) -> Result<(ir::ValueId, Type), ParseError> {
        if *self.peek_kind() == TokenKind::Minus {
            let span = self.peek().span;
            self.advance();
            let (v, ty) = self.parse_unary(ctx)?;
            if ty != Type::I64 {
                return Err(ParseError::TypeMismatch {
                    expected: Type::I64,
                    actual: ty,
                    span,
                });
            }
            let result = ctx
                .builder
                .emit(ctx.current_block, Type::I64, InstKind::Un(UnOp::Neg, v));
            Ok((result, Type::I64))
        } else {
            self.parse_primary(ctx)
        }
    }

    fn parse_primary(&mut self, ctx: &mut FnContext) -> Result<(ir::ValueId, Type), ParseError> {
        let span = self.peek().span;
        match self.peek_kind().clone() {
            TokenKind::Int(n) => {
                self.advance();
                Ok((
                    ctx.builder
                        .emit(ctx.current_block, Type::I64, InstKind::ConstI64(n)),
                    Type::I64,
                ))
            }
            TokenKind::True => {
                self.advance();
                Ok((
                    ctx.builder
                        .emit(ctx.current_block, Type::Bool, InstKind::ConstBool(true)),
                    Type::Bool,
                ))
            }
            TokenKind::False => {
                self.advance();
                Ok((
                    ctx.builder
                        .emit(ctx.current_block, Type::Bool, InstKind::ConstBool(false)),
                    Type::Bool,
                ))
            }
            TokenKind::LParen => {
                self.advance();
                let e = self.parse_expr(ctx)?;
                self.expect(&TokenKind::RParen)?;
                Ok(e)
            }
            TokenKind::Ident(name) => {
                self.advance();
                if *self.peek_kind() == TokenKind::LParen {
                    self.parse_call(ctx, name, span)
                } else {
                    let ty =
                        ctx.lookup_var(&name)
                            .ok_or_else(|| ParseError::UndefinedVariable {
                                name: name.clone(),
                                span,
                            })?;
                    Ok((ctx.builder.read_variable(&name, ctx.current_block, ty), ty))
                }
            }
            other => Err(ParseError::UnexpectedToken {
                expected: "an expression".to_string(),
                found: other,
                span,
            }),
        }
    }

    fn parse_call(
        &mut self,
        ctx: &mut FnContext,
        name: String,
        span: Span,
    ) -> Result<(ir::ValueId, Type), ParseError> {
        self.expect(&TokenKind::LParen)?;
        let mut args = Vec::new();
        let mut arg_types = Vec::new();
        if *self.peek_kind() != TokenKind::RParen {
            loop {
                let (v, ty) = self.parse_expr(ctx)?;
                args.push(v);
                arg_types.push(ty);
                if *self.peek_kind() == TokenKind::Comma {
                    self.advance();
                } else {
                    break;
                }
            }
        }
        self.expect(&TokenKind::RParen)?;

        let sig = ctx
            .sigs
            .get(&name)
            .ok_or_else(|| ParseError::UndefinedFunction {
                name: name.clone(),
                span,
            })?
            .clone();
        if sig.params.len() != args.len() {
            return Err(ParseError::ArgCountMismatch {
                func: name,
                expected: sig.params.len(),
                given: args.len(),
                span,
            });
        }
        for (given, expected) in arg_types.iter().zip(&sig.params) {
            if given != expected {
                return Err(ParseError::TypeMismatch {
                    expected: *expected,
                    actual: *given,
                    span,
                });
            }
        }
        let ret_ty = sig.ret_ty.unwrap_or(Type::I64); // v1 grammar always requires ->type
        let result = ctx.builder.emit(
            ctx.current_block,
            ret_ty,
            InstKind::Call { func: name, args },
        );
        Ok((result, ret_ty))
    }
}

/// Per-function parsing state threaded through statement/expression
/// parsing: the SSA builder, lexical scopes (name -> declared type,
/// innermost last), the block currently being appended to, the
/// function's return type, and the (already-fully-scanned) signature
/// table every `Call` is checked against.
struct FnContext {
    builder: SsaBuilder,
    scopes: Vec<HashMap<String, Type>>,
    current_block: ir::BlockId,
    ret_ty: Option<Type>,
    sigs: HashMap<String, FnSig>,
}

impl FnContext {
    fn lookup_var(&self, name: &str) -> Option<Type> {
        self.scopes.iter().rev().find_map(|s| s.get(name).copied())
    }
}
