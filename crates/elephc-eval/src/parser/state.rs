//! Purpose:
//! Parses tokenized eval fragments into EvalIR.
//! The parser state owns PHP statement and expression grammar for the runtime
//! eval subset after tokenization has completed.
//!
//! Called from:
//! - `crate::parser::parse_fragment()`.
//!
//! Key details:
//! - Namespace imports are tracked as parser state and restored across blocks.
//! - Unsupported PHP constructs fail with explicit parse statuses instead of
//!   partially lowering ambiguous syntax.

use crate::errors::EvalParseError;
use crate::eval_ir::{
    EvalArrayElement, EvalBinOp, EvalCallArg, EvalCatch, EvalClass, EvalClassMethod,
    EvalClassProperty, EvalConst, EvalExpr, EvalMagicConst, EvalMatchArm, EvalProgram, EvalStmt,
    EvalSwitchCase, EvalUnaryOp,
};
use crate::lexer::TokenKind;
use std::collections::HashMap;

/// Parses tokenized eval fragments into EvalIR.
pub(super) struct Parser {
    tokens: Vec<TokenKind>,
    pos: usize,
    source_len: usize,
    namespace: String,
    imports: NamespaceImports,
    allow_use_imports: bool,
}

/// A parsed PHP name plus whether it used a leading global namespace separator.
struct ParsedQualifiedName {
    name: String,
    absolute: bool,
}

/// Import alias tables active for the current namespace declaration region.
#[derive(Default)]
struct NamespaceImports {
    classes: HashMap<String, String>,
    functions: HashMap<String, String>,
    constants: HashMap<String, String>,
}

/// The `use` declaration namespace being imported.
#[derive(Copy, Clone, Eq, PartialEq)]
enum UseImportKind {
    Class,
    Function,
    Const,
}

impl NamespaceImports {
    /// Stores one class import under PHP's case-insensitive class alias key.
    fn insert_class(&mut self, alias: String, name: String) {
        self.classes.insert(alias.to_ascii_lowercase(), name);
    }

    /// Stores one function import under PHP's case-insensitive function alias key.
    fn insert_function(&mut self, alias: String, name: String) {
        self.functions.insert(alias.to_ascii_lowercase(), name);
    }

    /// Stores one constant import under PHP's case-sensitive constant alias key.
    fn insert_constant(&mut self, alias: String, name: String) {
        self.constants.insert(alias, name);
    }

    /// Resolves a class import, including aliases used as the first segment of a class name.
    fn resolve_class(&self, name: &str) -> Option<String> {
        let (first, tail) = split_first_name_segment(name);
        let imported = self.classes.get(&first.to_ascii_lowercase())?;
        Some(match tail {
            Some(tail) => format!("{imported}\\{tail}"),
            None => imported.clone(),
        })
    }

    /// Resolves an unqualified function alias.
    fn resolve_function(&self, name: &str) -> Option<&str> {
        self.functions
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }

    /// Resolves a case-sensitive unqualified constant alias.
    fn resolve_constant(&self, name: &str) -> Option<&str> {
        self.constants.get(name).map(String::as_str)
    }
}

impl Parser {
    /// Creates a parser over tokens produced from a source fragment.
    pub(super) fn new(tokens: Vec<TokenKind>, source_len: usize) -> Self {
        Self {
            tokens,
            pos: 0,
            source_len,
            namespace: String::new(),
            imports: NamespaceImports::default(),
            allow_use_imports: true,
        }
    }

    /// Parses a complete eval fragment until EOF.
    pub(super) fn parse_program(mut self) -> Result<EvalProgram, EvalParseError> {
        let mut statements = Vec::new();
        while !matches!(self.current(), TokenKind::Eof) {
            statements.extend(self.parse_stmt()?);
        }
        Ok(EvalProgram::new(self.source_len, statements))
    }

    /// Parses one source statement, expanding `unset($a, $b)` to one statement per variable.
    fn parse_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        match self.current() {
            TokenKind::Ident(name) if ident_eq(name, "break") => {
                self.advance();
                self.expect_semicolon()?;
                Ok(vec![EvalStmt::Break])
            }
            TokenKind::Ident(name) if ident_eq(name, "continue") => {
                self.advance();
                self.expect_semicolon()?;
                Ok(vec![EvalStmt::Continue])
            }
            TokenKind::Ident(name) if ident_eq(name, "do") => self.parse_do_while_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "echo") => {
                self.advance();
                let mut statements = vec![EvalStmt::Echo(self.parse_expr()?)];
                while self.consume(TokenKind::Comma) {
                    statements.push(EvalStmt::Echo(self.parse_expr()?));
                }
                self.expect_semicolon()?;
                Ok(statements)
            }
            TokenKind::Ident(name) if ident_eq(name, "for") => self.parse_for_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "foreach") => self.parse_foreach_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "class") => self.parse_class_decl_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "function") => self.parse_function_decl_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "global") => self.parse_global_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "if") => self.parse_if_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "namespace") => self.parse_namespace_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "return") => {
                self.advance();
                if self.consume_semicolon() {
                    return Ok(vec![EvalStmt::Return(None)]);
                }
                let expr = self.parse_expr()?;
                self.expect_semicolon()?;
                Ok(vec![EvalStmt::Return(Some(expr))])
            }
            TokenKind::Ident(name) if ident_eq(name, "static") => self.parse_static_var_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "switch") => self.parse_switch_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "throw") => self.parse_throw_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "try") => self.parse_try_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "unset") => self.parse_unset_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "use") && self.allow_use_imports => {
                self.parse_use_stmt()
            }
            TokenKind::Ident(name) if ident_eq(name, "use") => {
                Err(EvalParseError::UnsupportedConstruct)
            }
            TokenKind::Ident(name) if ident_eq(name, "while") => self.parse_while_stmt(),
            TokenKind::Ident(name) if is_unsupported_statement_keyword(name) => {
                Err(EvalParseError::UnsupportedConstruct)
            }
            TokenKind::PlusPlus | TokenKind::MinusMinus => self.parse_prefix_inc_dec_stmt(true),
            TokenKind::DollarIdent(_) if matches!(self.peek(), TokenKind::Arrow) => {
                self.parse_property_stmt(true)
            }
            TokenKind::DollarIdent(name) if matches!(self.peek(), TokenKind::LBracket) => {
                self.parse_array_set_stmt(name.clone())
            }
            TokenKind::DollarIdent(name)
                if matches!(self.peek(), TokenKind::PlusPlus | TokenKind::MinusMinus) =>
            {
                self.parse_postfix_inc_dec_stmt(name.clone(), true)
            }
            TokenKind::DollarIdent(name) if assignment_op(self.peek()).is_some() => {
                let name = name.clone();
                self.parse_var_store_stmt(name, true)
            }
            TokenKind::Eof => Err(EvalParseError::UnexpectedEof),
            _ => {
                let expr = self.parse_expr()?;
                self.expect_semicolon()?;
                Ok(vec![EvalStmt::Expr(expr)])
            }
        }
    }

    /// Parses `do { ... } while (expr);`.
    fn parse_do_while_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let body = self.parse_statement_body()?;
        if !matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "while")) {
            return Err(EvalParseError::UnexpectedToken);
        }
        self.advance();
        self.expect(TokenKind::LParen)?;
        let condition = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        self.expect_semicolon()?;
        Ok(vec![EvalStmt::DoWhile { body, condition }])
    }

    /// Parses `$name[index] = expr;` and `$name[] = expr;` eval writes.
    fn parse_array_set_stmt(&mut self, name: String) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LBracket)?;
        if self.consume(TokenKind::RBracket) {
            self.expect(TokenKind::Equal)?;
            let value = self.parse_expr()?;
            self.expect_semicolon()?;
            return Ok(vec![EvalStmt::ArrayAppendVar { name, value }]);
        }
        let index = self.parse_expr()?;
        self.expect(TokenKind::RBracket)?;
        self.expect(TokenKind::Equal)?;
        let value = self.parse_expr()?;
        self.expect_semicolon()?;
        Ok(vec![EvalStmt::ArraySetVar { name, index, value }])
    }

    /// Parses `for (init; condition; update) { ... }`.
    fn parse_for_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LParen)?;
        let init = self.parse_for_init_clause()?;
        self.expect_semicolon()?;
        let condition = if matches!(self.current(), TokenKind::Semicolon) {
            None
        } else {
            Some(self.parse_expr()?)
        };
        self.expect_semicolon()?;
        let update = self.parse_for_update_clause()?;
        let body = self.parse_statement_body()?;
        Ok(vec![EvalStmt::For {
            init,
            condition,
            update,
            body,
        }])
    }

    /// Parses `foreach (expr as $value) { ... }` or `foreach (expr as $key => $value) { ... }`.
    fn parse_foreach_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LParen)?;
        let array = self.parse_expr()?;
        if !matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "as")) {
            return Err(EvalParseError::UnexpectedToken);
        }
        self.advance();
        let TokenKind::DollarIdent(value_name) = self.current() else {
            return Err(EvalParseError::ExpectedVariable);
        };
        let value_name = value_name.clone();
        self.advance();
        let (key_name, value_name) = if matches!(self.current(), TokenKind::FatArrow) {
            self.advance();
            let TokenKind::DollarIdent(next_value_name) = self.current() else {
                return Err(EvalParseError::ExpectedVariable);
            };
            let key_name = value_name;
            let value_name = next_value_name.clone();
            self.advance();
            (Some(key_name), value_name)
        } else {
            (None, value_name)
        };
        self.expect(TokenKind::RParen)?;
        let body = self.parse_statement_body()?;
        Ok(vec![EvalStmt::Foreach {
            array,
            key_name,
            value_name,
            body,
        }])
    }

    /// Parses `class Name { ... }` declarations for dynamic class metadata.
    fn parse_class_decl_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let TokenKind::Ident(name) = self.current() else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let name = self.qualify_name_in_current_namespace(name);
        self.advance();
        self.expect(TokenKind::LBrace)?;
        let mut properties = Vec::new();
        let mut methods = Vec::new();
        while !self.consume(TokenKind::RBrace) {
            if matches!(self.current(), TokenKind::Eof) {
                return Err(EvalParseError::UnexpectedEof);
            }
            self.parse_class_member(&mut properties, &mut methods)?;
        }
        self.consume_semicolon();
        Ok(vec![EvalStmt::ClassDecl(EvalClass::new(
            name, properties, methods,
        ))])
    }

    /// Parses one public property or method from an eval class body.
    fn parse_class_member(
        &mut self,
        properties: &mut Vec<EvalClassProperty>,
        methods: &mut Vec<EvalClassMethod>,
    ) -> Result<(), EvalParseError> {
        let public = if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "public"))
        {
            self.advance();
            true
        } else if matches!(self.current(), TokenKind::Ident(name) if is_unsupported_class_member_modifier(name))
        {
            return Err(EvalParseError::UnsupportedConstruct);
        } else {
            false
        };

        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "function")) {
            methods.push(self.parse_class_method_decl()?);
            return Ok(());
        }

        if !public {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        properties.push(self.parse_class_property_decl()?);
        Ok(())
    }

    /// Parses `function name($param, ...) { ... }` inside a dynamic eval class.
    fn parse_class_method_decl(&mut self) -> Result<EvalClassMethod, EvalParseError> {
        self.advance();
        let TokenKind::Ident(name) = self.current() else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let name = name.clone();
        self.advance();
        self.expect(TokenKind::LParen)?;
        let params = self.parse_function_params()?;
        let body = self.parse_block()?;
        Ok(EvalClassMethod::new(name, params, body))
    }

    /// Parses one public property declaration with an optional initializer.
    fn parse_class_property_decl(&mut self) -> Result<EvalClassProperty, EvalParseError> {
        self.skip_optional_property_type()?;
        let TokenKind::DollarIdent(name) = self.current() else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let name = name.clone();
        self.advance();
        let default = if self.consume(TokenKind::Equal) {
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.expect_semicolon()?;
        Ok(EvalClassProperty::new(name, default))
    }

    /// Consumes a simple declared property type before the `$property` token.
    fn skip_optional_property_type(&mut self) -> Result<(), EvalParseError> {
        if matches!(self.current(), TokenKind::DollarIdent(_)) {
            return Ok(());
        }
        if self.consume(TokenKind::Question) && matches!(self.current(), TokenKind::DollarIdent(_))
        {
            return Err(EvalParseError::UnexpectedToken);
        }
        match self.current() {
            TokenKind::Ident(_) | TokenKind::Backslash => {
                let _ = self.parse_qualified_name()?;
                Ok(())
            }
            _ => Err(EvalParseError::UnexpectedToken),
        }
    }

    /// Parses `function name($param, ...) { ... }` declarations.
    fn parse_function_decl_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let TokenKind::Ident(name) = self.current() else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let name = self.qualify_name_in_current_namespace(name);
        self.advance();
        self.expect(TokenKind::LParen)?;
        let params = self.parse_function_params()?;
        let body = self.parse_block()?;
        Ok(vec![EvalStmt::FunctionDecl { name, params, body }])
    }

    /// Parses `namespace Name;` or `namespace Name { ... }` eval namespace blocks.
    fn parse_namespace_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let namespace = if self.consume(TokenKind::LBrace) {
            return self.parse_namespace_block(String::new());
        } else {
            self.parse_namespace_name()?
        };
        if self.consume_semicolon() {
            self.namespace = namespace;
            self.imports = NamespaceImports::default();
            return Ok(Vec::new());
        }
        self.expect(TokenKind::LBrace)?;
        self.parse_namespace_block(namespace)
    }

    /// Parses statements inside an already opened namespace block.
    fn parse_namespace_block(
        &mut self,
        namespace: String,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        let previous = std::mem::replace(&mut self.namespace, namespace);
        let previous_imports = std::mem::take(&mut self.imports);
        let previous_allow_use_imports = std::mem::replace(&mut self.allow_use_imports, true);
        let result = self.parse_block_contents();
        self.namespace = previous;
        self.imports = previous_imports;
        self.allow_use_imports = previous_allow_use_imports;
        result
    }

    /// Parses a namespace declaration name without a leading global separator.
    fn parse_namespace_name(&mut self) -> Result<String, EvalParseError> {
        let name = self.parse_qualified_name()?;
        if name.absolute {
            return Err(EvalParseError::UnexpectedToken);
        }
        Ok(name.name)
    }

    /// Parses PHP `use`, `use function`, and `use const` import declarations.
    fn parse_use_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let kind = self.parse_use_import_kind();

        loop {
            self.parse_use_import(kind)?;
            if !self.consume(TokenKind::Comma) {
                break;
            }
        }
        self.expect_semicolon()?;
        Ok(Vec::new())
    }

    /// Parses an optional top-level `function` or `const` use-import kind.
    fn parse_use_import_kind(&mut self) -> UseImportKind {
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "function")) {
            self.advance();
            UseImportKind::Function
        } else if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "const")) {
            self.advance();
            UseImportKind::Const
        } else {
            UseImportKind::Class
        }
    }

    /// Parses and registers one comma-separated import entry.
    fn parse_use_import(&mut self, kind: UseImportKind) -> Result<(), EvalParseError> {
        let (name, grouped) = self.parse_use_name_or_group_start()?;
        if grouped {
            return self.parse_grouped_use_imports(kind, name);
        }
        self.parse_use_alias_and_register(kind, name)
    }

    /// Parses a use-import name, stopping after a trailing namespace separator before `{`.
    fn parse_use_name_or_group_start(&mut self) -> Result<(String, bool), EvalParseError> {
        let _ = self.consume(TokenKind::Backslash);
        let TokenKind::Ident(first) = self.current() else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let mut name = first.clone();
        self.advance();
        while self.consume(TokenKind::Backslash) {
            if self.consume(TokenKind::LBrace) {
                return Ok((name, true));
            }
            let TokenKind::Ident(part) = self.current() else {
                return Err(EvalParseError::UnexpectedToken);
            };
            name.push('\\');
            name.push_str(part);
            self.advance();
        }
        Ok((name, false))
    }

    /// Parses all members inside a grouped namespace import declaration.
    fn parse_grouped_use_imports(
        &mut self,
        default_kind: UseImportKind,
        prefix: String,
    ) -> Result<(), EvalParseError> {
        if matches!(self.current(), TokenKind::RBrace) {
            return Err(EvalParseError::UnexpectedToken);
        }
        loop {
            let kind = self.parse_grouped_use_entry_kind(default_kind)?;
            let member = self.parse_grouped_use_member_name()?;
            let name = join_grouped_use_name(&prefix, &member);
            self.parse_use_alias_and_register(kind, name)?;
            if !self.consume(TokenKind::Comma) {
                break;
            }
            if self.consume(TokenKind::RBrace) {
                return Ok(());
            }
        }
        self.expect(TokenKind::RBrace)
    }

    /// Parses an optional per-entry grouped import kind, matching PHP's mixed group rules.
    fn parse_grouped_use_entry_kind(
        &mut self,
        default_kind: UseImportKind,
    ) -> Result<UseImportKind, EvalParseError> {
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "function")) {
            if default_kind != UseImportKind::Class {
                return Err(EvalParseError::UnexpectedToken);
            }
            self.advance();
            return Ok(UseImportKind::Function);
        }
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "const")) {
            if default_kind != UseImportKind::Class {
                return Err(EvalParseError::UnexpectedToken);
            }
            self.advance();
            return Ok(UseImportKind::Const);
        }
        Ok(default_kind)
    }

    /// Parses one non-absolute member name inside a grouped use declaration.
    fn parse_grouped_use_member_name(&mut self) -> Result<String, EvalParseError> {
        let name = self.parse_qualified_name()?;
        if name.absolute {
            return Err(EvalParseError::UnexpectedToken);
        }
        Ok(name.name)
    }

    /// Parses an optional alias and stores one namespace import.
    fn parse_use_alias_and_register(
        &mut self,
        kind: UseImportKind,
        name: String,
    ) -> Result<(), EvalParseError> {
        let alias = if matches!(
            self.current(),
            TokenKind::Ident(keyword) if ident_eq(keyword, "as")
        ) {
            self.advance();
            let TokenKind::Ident(alias) = self.current() else {
                return Err(EvalParseError::UnexpectedToken);
            };
            let alias = alias.clone();
            self.advance();
            alias
        } else {
            last_name_segment(&name).to_string()
        };

        match kind {
            UseImportKind::Class => self.imports.insert_class(alias, name),
            UseImportKind::Function => self.imports.insert_function(alias, name),
            UseImportKind::Const => self.imports.insert_constant(alias, name),
        }
        Ok(())
    }

    /// Parses `global $name, $other;` declarations in eval fragments.
    fn parse_global_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let mut vars = Vec::new();
        loop {
            let TokenKind::DollarIdent(name) = self.current() else {
                return Err(EvalParseError::ExpectedVariable);
            };
            vars.push(name.clone());
            self.advance();
            if !self.consume(TokenKind::Comma) {
                break;
            }
        }
        self.expect_semicolon()?;
        Ok(vec![EvalStmt::Global { vars }])
    }

    /// Parses `static $name = expr;` declarations in eval fragments.
    fn parse_static_var_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let TokenKind::DollarIdent(name) = self.current() else {
            return Err(EvalParseError::ExpectedVariable);
        };
        let name = name.clone();
        self.advance();
        self.expect(TokenKind::Equal)?;
        let init = self.parse_expr()?;
        self.expect_semicolon()?;
        Ok(vec![EvalStmt::StaticVar { name, init }])
    }

    /// Parses `throw expr;` statements in eval fragments.
    fn parse_throw_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let expr = self.parse_expr()?;
        self.expect_semicolon()?;
        Ok(vec![EvalStmt::Throw(expr)])
    }

    /// Parses `try { ... } catch (Type|Other $name) { ... } finally { ... }` statements.
    fn parse_try_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let body = self.parse_block()?;
        let mut catches = Vec::new();
        while matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "catch")) {
            catches.push(self.parse_catch_clause()?);
        }
        let finally_body = if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "finally"))
        {
            self.advance();
            self.parse_block()?
        } else {
            Vec::new()
        };
        if catches.is_empty() && finally_body.is_empty() {
            return Err(EvalParseError::UnexpectedToken);
        }
        Ok(vec![EvalStmt::Try {
            body,
            catches,
            finally_body,
        }])
    }

    /// Parses one `catch (ClassName|Other [$name]) { ... }` clause.
    fn parse_catch_clause(&mut self) -> Result<EvalCatch, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LParen)?;
        let class_names = self.parse_catch_types()?;
        let var_name = if let TokenKind::DollarIdent(var_name) = self.current() {
            let var_name = var_name.clone();
            self.advance();
            Some(var_name)
        } else {
            None
        };
        self.expect(TokenKind::RParen)?;
        let body = self.parse_block()?;
        Ok(EvalCatch {
            class_names,
            var_name,
            body,
        })
    }

    /// Parses one or more unioned catch types in source order.
    fn parse_catch_types(&mut self) -> Result<Vec<String>, EvalParseError> {
        let class_name = self.parse_qualified_name()?;
        let mut class_names = vec![self.resolve_class_name(class_name)];
        while self.consume(TokenKind::Pipe) {
            let class_name = self.parse_qualified_name()?;
            class_names.push(self.resolve_class_name(class_name));
        }
        Ok(class_names)
    }

    /// Parses a dynamic function declaration parameter list after `(`.
    fn parse_function_params(&mut self) -> Result<Vec<String>, EvalParseError> {
        let mut params = Vec::new();
        if self.consume(TokenKind::RParen) {
            return Ok(params);
        }
        loop {
            let TokenKind::DollarIdent(name) = self.current() else {
                return Err(EvalParseError::ExpectedVariable);
            };
            params.push(name.clone());
            self.advance();
            if !self.consume(TokenKind::Comma) {
                break;
            }
            if matches!(self.current(), TokenKind::RParen) {
                return Err(EvalParseError::ExpectedVariable);
            }
        }
        self.expect(TokenKind::RParen)?;
        Ok(params)
    }

    /// Parses the optional first clause of a `for` loop.
    fn parse_for_init_clause(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        if matches!(self.current(), TokenKind::Semicolon) {
            return Ok(Vec::new());
        }
        self.parse_for_clause_stmt()
    }

    /// Parses the optional update clause of a `for` loop.
    fn parse_for_update_clause(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        if self.consume(TokenKind::RParen) {
            return Ok(Vec::new());
        }
        let statements = self.parse_for_clause_stmt()?;
        self.expect(TokenKind::RParen)?;
        Ok(statements)
    }

    /// Parses one statement-like `for` clause without consuming a delimiter.
    fn parse_for_clause_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        match self.current() {
            TokenKind::PlusPlus | TokenKind::MinusMinus => self.parse_prefix_inc_dec_stmt(false),
            TokenKind::DollarIdent(name) if matches!(self.peek(), TokenKind::LBracket) => {
                self.parse_array_set_clause(name.clone())
            }
            TokenKind::DollarIdent(_) if matches!(self.peek(), TokenKind::Arrow) => {
                self.parse_property_stmt(false)
            }
            TokenKind::DollarIdent(name)
                if matches!(self.peek(), TokenKind::PlusPlus | TokenKind::MinusMinus) =>
            {
                self.parse_postfix_inc_dec_stmt(name.clone(), false)
            }
            TokenKind::DollarIdent(name) if assignment_op(self.peek()).is_some() => {
                let name = name.clone();
                self.parse_var_store_stmt(name, false)
            }
            _ => {
                let expr = self.parse_expr()?;
                Ok(vec![EvalStmt::Expr(expr)])
            }
        }
    }

    /// Parses `$name[index] = expr` and `$name[] = expr` in a `for` clause.
    fn parse_array_set_clause(&mut self, name: String) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LBracket)?;
        if self.consume(TokenKind::RBracket) {
            self.expect(TokenKind::Equal)?;
            let value = self.parse_expr()?;
            return Ok(vec![EvalStmt::ArrayAppendVar { name, value }]);
        }
        let index = self.parse_expr()?;
        self.expect(TokenKind::RBracket)?;
        self.expect(TokenKind::Equal)?;
        let value = self.parse_expr()?;
        Ok(vec![EvalStmt::ArraySetVar { name, index, value }])
    }

    /// Parses `$name = expr` and simple variable compound assignments.
    fn parse_var_store_stmt(
        &mut self,
        name: String,
        require_semicolon: bool,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let Some(op) = assignment_op(self.current()) else {
            return Err(EvalParseError::UnexpectedToken);
        };
        self.advance();
        if op.is_none() && matches!(self.current(), TokenKind::Ampersand) {
            self.advance();
            let TokenKind::DollarIdent(source) = self.current() else {
                return Err(EvalParseError::ExpectedVariable);
            };
            let source = source.clone();
            self.advance();
            if require_semicolon {
                self.expect_semicolon()?;
            }
            return Ok(vec![EvalStmt::ReferenceAssign {
                target: name,
                source,
            }]);
        }
        let value = self.parse_expr()?;
        if require_semicolon {
            self.expect_semicolon()?;
        }
        let value = assignment_value(&name, op, value);
        Ok(vec![EvalStmt::StoreVar { name, value }])
    }

    /// Parses prefix `++$name` and `--$name` as simple statement effects.
    fn parse_prefix_inc_dec_stmt(
        &mut self,
        require_semicolon: bool,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        let increment = matches!(self.current(), TokenKind::PlusPlus);
        self.advance();
        let TokenKind::DollarIdent(name) = self.current() else {
            return Err(EvalParseError::ExpectedVariable);
        };
        let name = name.clone();
        self.advance();
        if require_semicolon {
            self.expect_semicolon()?;
        }
        Ok(vec![inc_dec_store(name, increment)])
    }

    /// Parses postfix `$name++` and `$name--` as simple statement effects.
    fn parse_postfix_inc_dec_stmt(
        &mut self,
        name: String,
        require_semicolon: bool,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let increment = matches!(self.current(), TokenKind::PlusPlus);
        self.advance();
        if require_semicolon {
            self.expect_semicolon()?;
        }
        Ok(vec![inc_dec_store(name, increment)])
    }

    /// Parses `$object->property` as either an expression statement or property write.
    fn parse_property_stmt(
        &mut self,
        require_semicolon: bool,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        let target = self.parse_expr()?;
        if !self.consume(TokenKind::Equal) {
            if require_semicolon {
                self.expect_semicolon()?;
            }
            return Ok(vec![EvalStmt::Expr(target)]);
        }
        let EvalExpr::PropertyGet { object, property } = target else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let value = self.parse_expr()?;
        if require_semicolon {
            self.expect_semicolon()?;
        }
        Ok(vec![EvalStmt::PropertySet {
            object: *object,
            property,
            value,
        }])
    }

    /// Parses a complete `if` statement after consuming the `if` keyword.
    fn parse_if_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        Ok(vec![self.parse_if_after_keyword()?])
    }

    /// Parses the condition, then block, and optional else branch for an `if` chain.
    fn parse_if_after_keyword(&mut self) -> Result<EvalStmt, EvalParseError> {
        self.expect(TokenKind::LParen)?;
        let condition = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        let then_branch = self.parse_statement_body()?;
        let else_branch = self.parse_optional_else_branch()?;
        Ok(EvalStmt::If {
            condition,
            then_branch,
            else_branch,
        })
    }

    /// Parses `elseif`, `else if`, or `else` branches after an `if` body.
    fn parse_optional_else_branch(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "elseif")) {
            self.advance();
            return Ok(vec![self.parse_if_after_keyword()?]);
        }
        if !matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "else")) {
            return Ok(Vec::new());
        }
        self.advance();
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "if")) {
            self.advance();
            Ok(vec![self.parse_if_after_keyword()?])
        } else {
            self.parse_statement_body()
        }
    }

    /// Parses `switch (expr) { case expr: ... default: ... }`.
    fn parse_switch_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LParen)?;
        let expr = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        self.expect(TokenKind::LBrace)?;
        let mut cases = Vec::new();
        while !matches!(self.current(), TokenKind::RBrace) {
            if matches!(self.current(), TokenKind::Eof) {
                return Err(EvalParseError::UnexpectedEof);
            }
            cases.push(self.parse_switch_case()?);
        }
        self.expect(TokenKind::RBrace)?;
        Ok(vec![EvalStmt::Switch { expr, cases }])
    }

    /// Parses one `case` or `default` arm inside a switch body.
    fn parse_switch_case(&mut self) -> Result<EvalSwitchCase, EvalParseError> {
        let condition = if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "case"))
        {
            self.advance();
            let condition = self.parse_expr()?;
            Some(condition)
        } else if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "default")) {
            self.advance();
            None
        } else {
            return Err(EvalParseError::UnexpectedToken);
        };
        self.expect(TokenKind::Colon)?;
        let body = self.parse_switch_case_body()?;
        Ok(EvalSwitchCase { condition, body })
    }

    /// Parses case body statements until the next case boundary or switch close.
    fn parse_switch_case_body(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        let mut body = Vec::new();
        while !is_switch_case_boundary(self.current()) {
            if matches!(self.current(), TokenKind::Eof) {
                return Err(EvalParseError::UnexpectedEof);
            }
            body.extend(self.parse_stmt()?);
        }
        Ok(body)
    }

    /// Parses `unset($name[, ...]);`.
    fn parse_unset_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LParen)?;
        let mut statements = Vec::new();
        loop {
            let TokenKind::DollarIdent(name) = self.current() else {
                return Err(EvalParseError::ExpectedVariable);
            };
            statements.push(EvalStmt::UnsetVar { name: name.clone() });
            self.advance();
            if !self.consume(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RParen)?;
        self.expect_semicolon()?;
        Ok(statements)
    }

    /// Parses `while (expr) { ... }`.
    fn parse_while_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LParen)?;
        let condition = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        let body = self.parse_statement_body()?;
        Ok(vec![EvalStmt::While { condition, body }])
    }

    /// Parses either a brace-delimited block or one braceless statement body.
    fn parse_statement_body(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        if matches!(self.current(), TokenKind::LBrace) {
            self.parse_block()
        } else {
            self.parse_nested_stmt()
        }
    }

    /// Parses a brace-delimited statement block.
    fn parse_block(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.expect(TokenKind::LBrace)?;
        self.parse_nested_block_contents()
    }

    /// Parses one nested statement where import declarations are not legal.
    fn parse_nested_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        let previous = std::mem::replace(&mut self.allow_use_imports, false);
        let result = self.parse_stmt();
        self.allow_use_imports = previous;
        result
    }

    /// Parses a nested block while preserving active imports for name resolution.
    fn parse_nested_block_contents(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        let previous = std::mem::replace(&mut self.allow_use_imports, false);
        let result = self.parse_block_contents();
        self.allow_use_imports = previous;
        result
    }

    /// Parses statements until the closing brace for the current block.
    fn parse_block_contents(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        let mut statements = Vec::new();
        while !matches!(self.current(), TokenKind::RBrace) {
            if matches!(self.current(), TokenKind::Eof) {
                return Err(EvalParseError::UnexpectedEof);
            }
            statements.extend(self.parse_stmt()?);
        }
        self.expect(TokenKind::RBrace)?;
        Ok(statements)
    }

    /// Parses an expression using PHP-like logical, comparison, concatenation, and arithmetic precedence.
    fn parse_expr(&mut self) -> Result<EvalExpr, EvalParseError> {
        self.parse_keyword_or()
    }

    /// Parses PHP keyword `or`, whose precedence is lower than `xor`, `and`, and ternary.
    fn parse_keyword_or(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_keyword_xor()?;
        while matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "or")) {
            self.advance();
            let right = self.parse_keyword_xor()?;
            expr = EvalExpr::Binary {
                op: EvalBinOp::LogicalOr,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    /// Parses PHP keyword `xor`, whose operands are evaluated before boolean XOR.
    fn parse_keyword_xor(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_keyword_and()?;
        while matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "xor")) {
            self.advance();
            let right = self.parse_keyword_and()?;
            expr = EvalExpr::Binary {
                op: EvalBinOp::LogicalXor,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    /// Parses PHP keyword `and`, whose precedence is lower than ternary and `&&`.
    fn parse_keyword_and(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_ternary()?;
        while matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "and")) {
            self.advance();
            let right = self.parse_ternary()?;
            expr = EvalExpr::Binary {
                op: EvalBinOp::LogicalAnd,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    /// Parses PHP ternary expressions, including the short `expr ?: fallback` form.
    fn parse_ternary(&mut self) -> Result<EvalExpr, EvalParseError> {
        let condition = self.parse_null_coalesce()?;
        if !self.consume(TokenKind::Question) {
            return Ok(condition);
        }
        let then_branch = if self.consume(TokenKind::Colon) {
            None
        } else {
            let expr = self.parse_expr()?;
            self.expect(TokenKind::Colon)?;
            Some(Box::new(expr))
        };
        let else_branch = self.parse_expr()?;
        Ok(EvalExpr::Ternary {
            condition: Box::new(condition),
            then_branch,
            else_branch: Box::new(else_branch),
        })
    }

    /// Parses right-associative null coalescing below logical OR and above ternary.
    fn parse_null_coalesce(&mut self) -> Result<EvalExpr, EvalParseError> {
        let value = self.parse_logical_or()?;
        if !self.consume(TokenKind::QuestionQuestion) {
            return Ok(value);
        }
        let default = self.parse_null_coalesce()?;
        Ok(EvalExpr::NullCoalesce {
            value: Box::new(value),
            default: Box::new(default),
        })
    }

    /// Parses left-associative logical OR with lower precedence than logical AND.
    fn parse_logical_or(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_logical_and()?;
        while self.consume(TokenKind::OrOr) {
            let right = self.parse_logical_and()?;
            expr = EvalExpr::Binary {
                op: EvalBinOp::LogicalOr,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    /// Parses left-associative logical AND with lower precedence than equality.
    fn parse_logical_and(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_bit_or()?;
        while self.consume(TokenKind::AndAnd) {
            let right = self.parse_bit_or()?;
            expr = EvalExpr::Binary {
                op: EvalBinOp::LogicalAnd,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    /// Parses left-associative bitwise OR with lower precedence than bitwise XOR.
    fn parse_bit_or(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_bit_xor()?;
        while self.consume(TokenKind::Pipe) {
            let right = self.parse_bit_xor()?;
            expr = EvalExpr::Binary {
                op: EvalBinOp::BitOr,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    /// Parses left-associative bitwise XOR with lower precedence than bitwise AND.
    fn parse_bit_xor(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_bit_and()?;
        while self.consume(TokenKind::Caret) {
            let right = self.parse_bit_and()?;
            expr = EvalExpr::Binary {
                op: EvalBinOp::BitXor,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    /// Parses left-associative bitwise AND with lower precedence than equality.
    fn parse_bit_and(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_equality()?;
        while self.consume(TokenKind::Ampersand) {
            let right = self.parse_equality()?;
            expr = EvalExpr::Binary {
                op: EvalBinOp::BitAnd,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    /// Parses left-associative equality and inequality comparisons.
    fn parse_equality(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_ordering()?;
        loop {
            let op = if self.consume(TokenKind::EqualEqual) {
                EvalBinOp::LooseEq
            } else if self.consume(TokenKind::NotEqual) {
                EvalBinOp::LooseNotEq
            } else if self.consume(TokenKind::EqualEqualEqual) {
                EvalBinOp::StrictEq
            } else if self.consume(TokenKind::NotEqualEqual) {
                EvalBinOp::StrictNotEq
            } else {
                break;
            };
            let right = self.parse_ordering()?;
            expr = EvalExpr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    /// Parses left-associative ordered comparisons.
    fn parse_ordering(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_shift()?;
        loop {
            let op = if self.consume(TokenKind::Less) {
                EvalBinOp::Lt
            } else if self.consume(TokenKind::LessEqual) {
                EvalBinOp::LtEq
            } else if self.consume(TokenKind::Greater) {
                EvalBinOp::Gt
            } else if self.consume(TokenKind::GreaterEqual) {
                EvalBinOp::GtEq
            } else if self.consume(TokenKind::Spaceship) {
                EvalBinOp::Spaceship
            } else {
                break;
            };
            let right = self.parse_shift()?;
            expr = EvalExpr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    /// Parses left-associative integer shift operators.
    fn parse_shift(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_concat()?;
        loop {
            let op = if self.consume(TokenKind::LessLess) {
                EvalBinOp::ShiftLeft
            } else if self.consume(TokenKind::GreaterGreater) {
                EvalBinOp::ShiftRight
            } else {
                break;
            };
            let right = self.parse_concat()?;
            expr = EvalExpr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    /// Parses left-associative string concatenation.
    fn parse_concat(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_add()?;
        while self.consume(TokenKind::Dot) {
            let right = self.parse_add()?;
            expr = EvalExpr::Binary {
                op: EvalBinOp::Concat,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    /// Parses left-associative numeric addition and subtraction.
    fn parse_add(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_mul()?;
        loop {
            let op = if self.consume(TokenKind::Plus) {
                EvalBinOp::Add
            } else if self.consume(TokenKind::Minus) {
                EvalBinOp::Sub
            } else {
                break;
            };
            let right = self.parse_mul()?;
            expr = EvalExpr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    /// Parses left-associative numeric multiplication, division, and modulo.
    fn parse_mul(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_unary()?;
        loop {
            let op = if self.consume(TokenKind::Star) {
                EvalBinOp::Mul
            } else if self.consume(TokenKind::Slash) {
                EvalBinOp::Div
            } else if self.consume(TokenKind::Percent) {
                EvalBinOp::Mod
            } else {
                break;
            };
            let right = self.parse_unary()?;
            expr = EvalExpr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    /// Parses right-associative unary prefix expressions.
    fn parse_unary(&mut self) -> Result<EvalExpr, EvalParseError> {
        if self.consume(TokenKind::Plus) {
            let expr = self.parse_unary()?;
            return Ok(EvalExpr::Unary {
                op: EvalUnaryOp::Plus,
                expr: Box::new(expr),
            });
        }
        if self.consume(TokenKind::Minus) {
            let expr = self.parse_unary()?;
            return Ok(EvalExpr::Unary {
                op: EvalUnaryOp::Negate,
                expr: Box::new(expr),
            });
        }
        if self.consume(TokenKind::Bang) {
            let expr = self.parse_unary()?;
            return Ok(EvalExpr::Unary {
                op: EvalUnaryOp::LogicalNot,
                expr: Box::new(expr),
            });
        }
        if self.consume(TokenKind::Tilde) {
            let expr = self.parse_unary()?;
            return Ok(EvalExpr::Unary {
                op: EvalUnaryOp::BitNot,
                expr: Box::new(expr),
            });
        }
        self.parse_power()
    }

    /// Parses right-associative exponentiation with higher precedence than unary prefix operators.
    fn parse_power(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_postfix()?;
        if self.consume(TokenKind::StarStar) {
            let right = self.parse_unary()?;
            expr = EvalExpr::Binary {
                op: EvalBinOp::Pow,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    /// Parses postfix array reads, property reads, method calls, and dynamic calls.
    fn parse_postfix(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_primary()?;
        loop {
            if matches!(self.current(), TokenKind::LParen) {
                let args = self.parse_call_args()?;
                expr = EvalExpr::DynamicCall {
                    callee: Box::new(expr),
                    args,
                };
                continue;
            }
            if self.consume(TokenKind::LBracket) {
                let index = self.parse_expr()?;
                self.expect(TokenKind::RBracket)?;
                expr = EvalExpr::ArrayGet {
                    array: Box::new(expr),
                    index: Box::new(index),
                };
                continue;
            }
            if self.consume(TokenKind::Arrow) {
                let TokenKind::Ident(member) = self.current() else {
                    return Err(EvalParseError::UnexpectedToken);
                };
                let member = member.clone();
                self.advance();
                if matches!(self.current(), TokenKind::LParen) {
                    let args = self.parse_call_args()?;
                    expr = EvalExpr::MethodCall {
                        object: Box::new(expr),
                        method: member.to_ascii_lowercase(),
                        args,
                    };
                } else {
                    expr = EvalExpr::PropertyGet {
                        object: Box::new(expr),
                        property: member,
                    };
                }
                continue;
            }
            break;
        }
        Ok(expr)
    }

    /// Parses primary expressions supported by the initial eval subset.
    fn parse_primary(&mut self) -> Result<EvalExpr, EvalParseError> {
        match self.current() {
            TokenKind::Int(value) => {
                let value = *value;
                self.advance();
                Ok(EvalExpr::Const(EvalConst::Int(value)))
            }
            TokenKind::Float(value) => {
                let value = *value;
                self.advance();
                Ok(EvalExpr::Const(EvalConst::Float(value)))
            }
            TokenKind::String(value) => {
                let value = value.clone();
                self.advance();
                Ok(EvalExpr::Const(EvalConst::String(value)))
            }
            TokenKind::DollarIdent(name) => {
                let name = name.clone();
                self.advance();
                Ok(EvalExpr::LoadVar(name))
            }
            TokenKind::Magic(EvalMagicConst::Namespace) => {
                let namespace = self.namespace.clone();
                self.advance();
                Ok(EvalExpr::Const(EvalConst::String(namespace)))
            }
            TokenKind::Magic(magic) => {
                let magic = magic.clone();
                self.advance();
                Ok(EvalExpr::Magic(magic))
            }
            TokenKind::Ident(name) if ident_eq(name, "null") => {
                self.advance();
                Ok(EvalExpr::Const(EvalConst::Null))
            }
            TokenKind::Ident(name) if ident_eq(name, "true") => {
                self.advance();
                Ok(EvalExpr::Const(EvalConst::Bool(true)))
            }
            TokenKind::Ident(name) if ident_eq(name, "false") => {
                self.advance();
                Ok(EvalExpr::Const(EvalConst::Bool(false)))
            }
            TokenKind::Ident(name) if ident_eq(name, "print") => {
                self.advance();
                let expr = self.parse_expr()?;
                Ok(EvalExpr::Print(Box::new(expr)))
            }
            TokenKind::Ident(_) if self.current_starts_legacy_array_literal() => {
                self.parse_legacy_array_literal()
            }
            TokenKind::Ident(name) if is_include_construct_name(name) => self.parse_include_expr(),
            TokenKind::Ident(name) if ident_eq(name, "match") => self.parse_match_expr(),
            TokenKind::Ident(name) if ident_eq(name, "new") => self.parse_new_object_expr(),
            TokenKind::Ident(name) if is_unsupported_expression_keyword(name) => {
                Err(EvalParseError::UnsupportedConstruct)
            }
            TokenKind::Backslash => self.parse_qualified_name_expr(),
            TokenKind::Ident(_) if matches!(self.peek(), TokenKind::Backslash) => {
                self.parse_qualified_name_expr()
            }
            TokenKind::Ident(name) if matches!(self.peek(), TokenKind::LParen) => {
                self.parse_call_expr(name.clone())
            }
            TokenKind::Ident(name) => {
                let name = name.clone();
                self.advance();
                Ok(self.const_fetch_expr(name))
            }
            TokenKind::LBracket => self.parse_array_literal(),
            TokenKind::LParen => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(TokenKind::RParen)?;
                Ok(expr)
            }
            TokenKind::Eof => Err(EvalParseError::UnexpectedEof),
            _ => Err(EvalParseError::UnexpectedToken),
        }
    }

    /// Parses PHP include/require expression constructs and their path expression.
    fn parse_include_expr(&mut self) -> Result<EvalExpr, EvalParseError> {
        let TokenKind::Ident(name) = self.current() else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let required = ident_eq(name, "require") || ident_eq(name, "require_once");
        let once = ident_eq(name, "include_once") || ident_eq(name, "require_once");
        self.advance();
        let path = if self.consume(TokenKind::LParen) {
            let path = self.parse_expr()?;
            self.expect(TokenKind::RParen)?;
            path
        } else {
            self.parse_expr()?
        };
        Ok(EvalExpr::Include {
            path: Box::new(path),
            required,
            once,
        })
    }

    /// Parses `match (expr) { pattern, other => value, default => fallback }`.
    fn parse_match_expr(&mut self) -> Result<EvalExpr, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LParen)?;
        let subject = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        self.expect(TokenKind::LBrace)?;

        let mut arms = Vec::new();
        let mut default = None;
        while !self.consume(TokenKind::RBrace) {
            if matches!(self.current(), TokenKind::Eof) {
                return Err(EvalParseError::UnexpectedEof);
            }
            if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "default")) {
                self.advance();
                self.expect(TokenKind::FatArrow)?;
                default = Some(Box::new(self.parse_expr()?));
            } else {
                arms.push(self.parse_match_arm()?);
            }
            if self.consume(TokenKind::Comma) {
                continue;
            }
            self.expect(TokenKind::RBrace)?;
            break;
        }

        Ok(EvalExpr::Match {
            subject: Box::new(subject),
            arms,
            default,
        })
    }

    /// Parses one non-default `match` arm and its comma-separated pattern list.
    fn parse_match_arm(&mut self) -> Result<EvalMatchArm, EvalParseError> {
        let mut patterns = Vec::new();
        loop {
            patterns.push(self.parse_expr()?);
            if !self.consume(TokenKind::Comma) {
                break;
            }
            if matches!(self.current(), TokenKind::FatArrow) {
                return Err(EvalParseError::UnexpectedToken);
            }
            if matches!(self.current(), TokenKind::Eof | TokenKind::RBrace) {
                return Err(EvalParseError::UnexpectedToken);
            }
        }
        self.expect(TokenKind::FatArrow)?;
        let value = self.parse_expr()?;
        Ok(EvalMatchArm { patterns, value })
    }

    /// Parses a function-like call expression and its source-order arguments.
    fn parse_call_expr(&mut self, name: String) -> Result<EvalExpr, EvalParseError> {
        self.advance();
        let args = self.parse_call_args()?;
        Ok(self.call_expr(name, args))
    }

    /// Parses an explicitly qualified call or constant-fetch expression.
    fn parse_qualified_name_expr(&mut self) -> Result<EvalExpr, EvalParseError> {
        let name = self.parse_qualified_name()?;
        let name = self.resolve_qualified_name(name);
        if matches!(self.current(), TokenKind::LParen) {
            let args = self.parse_call_args()?;
            return Ok(EvalExpr::Call {
                name: name.to_ascii_lowercase(),
                args,
            });
        }
        Ok(EvalExpr::ConstFetch(name))
    }

    /// Parses `new ClassName(...)` expressions in eval fragments.
    fn parse_new_object_expr(&mut self) -> Result<EvalExpr, EvalParseError> {
        self.advance();
        let class_name = self.parse_qualified_name()?;
        let class_name = self.resolve_class_name(class_name);
        let args = self.parse_call_args()?;
        Ok(EvalExpr::NewObject { class_name, args })
    }

    /// Parses a simple or explicitly qualified PHP name.
    fn parse_qualified_name(&mut self) -> Result<ParsedQualifiedName, EvalParseError> {
        let absolute = self.consume(TokenKind::Backslash);
        let TokenKind::Ident(first) = self.current() else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let mut name = first.clone();
        self.advance();
        while self.consume(TokenKind::Backslash) {
            let TokenKind::Ident(part) = self.current() else {
                return Err(EvalParseError::UnexpectedToken);
            };
            name.push('\\');
            name.push_str(part);
            self.advance();
        }
        Ok(ParsedQualifiedName { name, absolute })
    }

    /// Builds a call expression, adding namespace fallback for unqualified names.
    fn call_expr(&self, name: String, args: Vec<EvalCallArg>) -> EvalExpr {
        if let Some(imported) = self.imports.resolve_function(&name) {
            return EvalExpr::Call {
                name: imported.to_ascii_lowercase(),
                args,
            };
        }
        let fallback_name = name.to_ascii_lowercase();
        if self.namespace.is_empty() {
            EvalExpr::Call {
                name: fallback_name,
                args,
            }
        } else {
            EvalExpr::NamespacedCall {
                name: self
                    .qualify_name_in_current_namespace(&name)
                    .to_ascii_lowercase(),
                fallback_name,
                args,
            }
        }
    }

    /// Builds a constant fetch expression, adding namespace fallback for unqualified names.
    fn const_fetch_expr(&self, name: String) -> EvalExpr {
        if let Some(imported) = self.imports.resolve_constant(&name) {
            return EvalExpr::ConstFetch(imported.to_string());
        }
        if self.namespace.is_empty() {
            EvalExpr::ConstFetch(name)
        } else {
            EvalExpr::NamespacedConstFetch {
                name: self.qualify_name_in_current_namespace(&name),
                fallback_name: name,
            }
        }
    }

    /// Prefixes a name with the parser's current namespace when one is active.
    fn qualify_name_in_current_namespace(&self, name: &str) -> String {
        if self.namespace.is_empty() {
            name.to_string()
        } else {
            format!("{}\\{}", self.namespace, name)
        }
    }

    /// Resolves a class name through active imports before namespace qualification.
    fn resolve_class_name(&self, name: ParsedQualifiedName) -> String {
        if name.absolute {
            return name.name;
        }
        if let Some(imported) = self.imports.resolve_class(&name.name) {
            return imported;
        }
        self.resolve_qualified_name(name)
    }

    /// Resolves a parsed PHP name according to the current namespace.
    fn resolve_qualified_name(&self, name: ParsedQualifiedName) -> String {
        if name.absolute || self.namespace.is_empty() {
            name.name
        } else {
            self.qualify_name_in_current_namespace(&name.name)
        }
    }

    /// Parses a parenthesized source-order argument list.
    fn parse_call_args(&mut self) -> Result<Vec<EvalCallArg>, EvalParseError> {
        self.expect(TokenKind::LParen)?;
        let mut args = Vec::new();
        if self.consume(TokenKind::RParen) {
            return Ok(args);
        }
        loop {
            args.push(self.parse_call_arg()?);
            if !self.consume(TokenKind::Comma) {
                break;
            }
            if self.consume(TokenKind::RParen) {
                return Ok(args);
            }
        }
        self.expect(TokenKind::RParen)?;
        Ok(args)
    }

    /// Parses one positional or named argument within a call argument list.
    fn parse_call_arg(&mut self) -> Result<EvalCallArg, EvalParseError> {
        if self.consume(TokenKind::Ellipsis) {
            return self.parse_expr().map(EvalCallArg::spread);
        }
        if matches!(self.peek(), TokenKind::Colon) {
            if let TokenKind::Ident(name) = self.current() {
                let name = name.clone();
                self.advance();
                self.expect(TokenKind::Colon)?;
                let value = self.parse_expr()?;
                return Ok(EvalCallArg::named(name, value));
            }
        }
        self.parse_expr().map(EvalCallArg::positional)
    }

    /// Parses an array literal with source-order optional key/value element expressions.
    fn parse_array_literal(&mut self) -> Result<EvalExpr, EvalParseError> {
        self.expect(TokenKind::LBracket)?;
        self.parse_array_elements_until(TokenKind::RBracket)
    }

    /// Parses PHP's legacy `array(...)` literal into the same EvalIR node as `[...]`.
    fn parse_legacy_array_literal(&mut self) -> Result<EvalExpr, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LParen)?;
        self.parse_array_elements_until(TokenKind::RParen)
    }

    /// Returns whether the current token starts PHP's legacy `array(...)` literal syntax.
    fn current_starts_legacy_array_literal(&self) -> bool {
        matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "array"))
            && matches!(self.peek(), TokenKind::LParen)
    }

    /// Parses comma-separated array elements until the supplied closing delimiter.
    fn parse_array_elements_until(&mut self, close: TokenKind) -> Result<EvalExpr, EvalParseError> {
        let mut elements = Vec::new();
        if self.consume(close.clone()) {
            return Ok(EvalExpr::Array(elements));
        }
        loop {
            let first = self.parse_expr()?;
            if self.consume(TokenKind::FatArrow) {
                let value = self.parse_expr()?;
                elements.push(EvalArrayElement::KeyValue { key: first, value });
            } else {
                elements.push(EvalArrayElement::Value(first));
            }
            if !self.consume(TokenKind::Comma) {
                break;
            }
            if self.consume(close.clone()) {
                return Ok(EvalExpr::Array(elements));
            }
        }
        self.expect(close)?;
        Ok(EvalExpr::Array(elements))
    }

    /// Consumes `expected` or returns a parse error.
    fn expect(&mut self, expected: TokenKind) -> Result<(), EvalParseError> {
        if self.consume(expected) {
            Ok(())
        } else {
            Err(EvalParseError::UnexpectedToken)
        }
    }

    /// Consumes a semicolon or returns the semicolon-specific parse error.
    fn expect_semicolon(&mut self) -> Result<(), EvalParseError> {
        if self.consume_semicolon() {
            Ok(())
        } else {
            Err(EvalParseError::ExpectedSemicolon)
        }
    }

    /// Consumes a semicolon if present.
    fn consume_semicolon(&mut self) -> bool {
        self.consume(TokenKind::Semicolon)
    }

    /// Consumes `expected` if the current token matches it.
    fn consume(&mut self, expected: TokenKind) -> bool {
        if *self.current() == expected {
            self.advance();
            true
        } else {
            false
        }
    }

    /// Returns the current token.
    fn current(&self) -> &TokenKind {
        self.tokens.get(self.pos).unwrap_or(&TokenKind::Eof)
    }

    /// Returns the next token without advancing.
    fn peek(&self) -> &TokenKind {
        self.tokens.get(self.pos + 1).unwrap_or(&TokenKind::Eof)
    }

    /// Advances to the next token.
    fn advance(&mut self) {
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
    }
}
/// Returns true when the current token closes or starts a switch case arm.
fn is_switch_case_boundary(token: &TokenKind) -> bool {
    matches!(token, TokenKind::RBrace)
        || matches!(token, TokenKind::Ident(name) if ident_eq(name, "case") || ident_eq(name, "default"))
}

/// Maps simple variable assignment tokens to an optional compound EvalIR operator.
fn assignment_op(token: &TokenKind) -> Option<Option<EvalBinOp>> {
    match token {
        TokenKind::Equal => Some(None),
        TokenKind::PlusEqual => Some(Some(EvalBinOp::Add)),
        TokenKind::MinusEqual => Some(Some(EvalBinOp::Sub)),
        TokenKind::StarEqual => Some(Some(EvalBinOp::Mul)),
        TokenKind::StarStarEqual => Some(Some(EvalBinOp::Pow)),
        TokenKind::SlashEqual => Some(Some(EvalBinOp::Div)),
        TokenKind::PercentEqual => Some(Some(EvalBinOp::Mod)),
        TokenKind::AmpEqual => Some(Some(EvalBinOp::BitAnd)),
        TokenKind::PipeEqual => Some(Some(EvalBinOp::BitOr)),
        TokenKind::CaretEqual => Some(Some(EvalBinOp::BitXor)),
        TokenKind::LessLessEqual => Some(Some(EvalBinOp::ShiftLeft)),
        TokenKind::GreaterGreaterEqual => Some(Some(EvalBinOp::ShiftRight)),
        TokenKind::DotEqual => Some(Some(EvalBinOp::Concat)),
        _ => None,
    }
}

/// Builds the assigned value expression for plain and compound variable assignment.
fn assignment_value(name: &str, op: Option<EvalBinOp>, value: EvalExpr) -> EvalExpr {
    match op {
        Some(op) => EvalExpr::Binary {
            op,
            left: Box::new(EvalExpr::LoadVar(name.to_string())),
            right: Box::new(value),
        },
        None => value,
    }
}

/// Builds the StoreVar statement for a simple variable increment or decrement.
pub(super) fn inc_dec_store(name: String, increment: bool) -> EvalStmt {
    EvalStmt::StoreVar {
        value: EvalExpr::Binary {
            op: if increment {
                EvalBinOp::Add
            } else {
                EvalBinOp::Sub
            },
            left: Box::new(EvalExpr::LoadVar(name.clone())),
            right: Box::new(EvalExpr::Const(EvalConst::Int(1))),
        },
        name,
    }
}

/// Compares a source identifier to a PHP keyword using ASCII case-insensitive rules.
fn ident_eq(actual: &str, expected: &str) -> bool {
    actual.eq_ignore_ascii_case(expected)
}

/// Returns true for PHP statement forms that the eval subset intentionally does not parse yet.
fn is_unsupported_statement_keyword(name: &str) -> bool {
    ["enum", "interface", "trait"]
        .iter()
        .any(|keyword| ident_eq(name, keyword))
}

/// Returns true for class member modifiers outside the current eval class subset.
fn is_unsupported_class_member_modifier(name: &str) -> bool {
    ["private", "protected", "static", "abstract", "final"]
        .iter()
        .any(|modifier| ident_eq(name, modifier))
}

/// Returns true when an identifier is an include/require expression construct.
fn is_include_construct_name(name: &str) -> bool {
    ["include", "include_once", "require", "require_once"]
        .iter()
        .any(|keyword| ident_eq(name, keyword))
}

/// Returns the first namespace segment and the optional remaining suffix.
fn split_first_name_segment(name: &str) -> (&str, Option<&str>) {
    name.split_once('\\')
        .map_or((name, None), |(first, tail)| (first, Some(tail)))
}

/// Returns the final segment of a PHP qualified name.
fn last_name_segment(name: &str) -> &str {
    name.rsplit('\\').next().unwrap_or(name)
}

/// Combines a grouped use prefix with one relative member name.
fn join_grouped_use_name(prefix: &str, member: &str) -> String {
    format!("{prefix}\\{member}")
}

/// Returns true for PHP expression forms that the eval subset intentionally does not parse yet.
fn is_unsupported_expression_keyword(name: &str) -> bool {
    ["clone", "yield"]
        .iter()
        .any(|keyword| ident_eq(name, keyword))
}
