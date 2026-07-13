//! Purpose:
//! Parses static member and object-construction expressions and resolves PHP
//! class, function, and constant names.
//!
//! Called from:
//! - Primary and postfix parsing for qualified names, `new`, and `Class::member`.
//!
//! Key details:
//! - Namespace/import fallback and reserved class-reference rules stay shared.

use super::*;

impl Parser {

    /// Parses `Class::$property` and `Class::method(...)` expressions.
    pub(in crate::parser) fn parse_static_member_expr(
        &mut self,
        class_name: String,
    ) -> Result<EvalExpr, EvalParseError> {
        match self.current() {
            TokenKind::DollarIdent(property) => {
                let property = property.clone();
                self.advance();
                if matches!(self.current(), TokenKind::LParen) {
                    if self.consume_first_class_callable_marker() {
                        return Ok(EvalExpr::StaticMethodCallable {
                            class_name,
                            method: Box::new(EvalExpr::LoadVar(property)),
                        });
                    }
                    let args = self.parse_call_args()?;
                    return Ok(EvalExpr::DynamicStaticMethodCall {
                        class_name: Box::new(EvalExpr::ClassNameFetch { class_name }),
                        method: Box::new(EvalExpr::LoadVar(property)),
                        args,
                    });
                }
                Ok(EvalExpr::StaticPropertyGet {
                    class_name,
                    property,
                })
            }
            TokenKind::DollarLBrace => {
                self.advance();
                let property = self.parse_expr()?;
                self.expect(TokenKind::RBrace)?;
                Ok(EvalExpr::DynamicStaticPropertyNameGet {
                    class_name: Box::new(EvalExpr::ClassNameFetch { class_name }),
                    property: Box::new(property),
                })
            }
            TokenKind::Ident(name)
                if ident_eq(name, "class") && !matches!(self.peek(), TokenKind::LParen) =>
            {
                self.advance();
                Ok(EvalExpr::ClassNameFetch { class_name })
            }
            TokenKind::Ident(method) if matches!(self.peek(), TokenKind::LParen) => {
                let method = method.clone();
                self.advance();
                if self.consume_first_class_callable_marker() {
                    return Ok(EvalExpr::StaticMethodCallable {
                        class_name,
                        method: Box::new(EvalExpr::Const(EvalConst::String(method))),
                    });
                }
                let args = self.parse_call_args()?;
                Ok(EvalExpr::StaticMethodCall {
                    class_name,
                    method,
                    args,
                })
            }
            TokenKind::LBrace => {
                self.advance();
                let member = self.parse_expr()?;
                self.expect(TokenKind::RBrace)?;
                if matches!(self.current(), TokenKind::LParen) {
                    if self.consume_first_class_callable_marker() {
                        return Ok(EvalExpr::StaticMethodCallable {
                            class_name,
                            method: Box::new(member),
                        });
                    }
                    let args = self.parse_call_args()?;
                    return Ok(EvalExpr::DynamicStaticMethodCall {
                        class_name: Box::new(EvalExpr::ClassNameFetch { class_name }),
                        method: Box::new(member),
                        args,
                    });
                }
                Ok(EvalExpr::DynamicClassConstantNameFetch {
                    class_name: Box::new(EvalExpr::ClassNameFetch { class_name }),
                    constant: Box::new(member),
                })
            }
            TokenKind::Ident(constant) => {
                let constant = constant.clone();
                self.advance();
                Ok(EvalExpr::ClassConstantFetch {
                    class_name,
                    constant,
                })
            }
            _ => Err(EvalParseError::UnsupportedConstruct),
        }
    }

    /// Parses `$class::member` expressions whose static receiver is runtime-valued.
    pub(super) fn parse_dynamic_static_member_expr(
        &mut self,
        class_name: EvalExpr,
    ) -> Result<EvalExpr, EvalParseError> {
        match self.current() {
            TokenKind::DollarIdent(member) => {
                let member = member.clone();
                self.advance();
                if matches!(self.current(), TokenKind::LParen) {
                    if self.consume_first_class_callable_marker() {
                        return Ok(Self::dynamic_static_method_callable_expr(
                            class_name,
                            EvalExpr::LoadVar(member),
                        ));
                    }
                    let args = self.parse_call_args()?;
                    return Ok(EvalExpr::DynamicStaticMethodCall {
                        class_name: Box::new(class_name),
                        method: Box::new(EvalExpr::LoadVar(member)),
                        args,
                    });
                }
                Ok(EvalExpr::DynamicStaticPropertyGet {
                    class_name: Box::new(class_name),
                    property: member,
                })
            }
            TokenKind::DollarLBrace => {
                self.advance();
                let property = self.parse_expr()?;
                self.expect(TokenKind::RBrace)?;
                Ok(EvalExpr::DynamicStaticPropertyNameGet {
                    class_name: Box::new(class_name),
                    property: Box::new(property),
                })
            }
            TokenKind::Ident(name)
                if ident_eq(name, "class") && !matches!(self.peek(), TokenKind::LParen) =>
            {
                self.advance();
                Ok(EvalExpr::DynamicClassNameFetch {
                    class_name: Box::new(class_name),
                })
            }
            TokenKind::Ident(method) if matches!(self.peek(), TokenKind::LParen) => {
                let method = method.clone();
                self.advance();
                if self.consume_first_class_callable_marker() {
                    return Ok(Self::dynamic_static_method_callable_expr(
                        class_name,
                        EvalExpr::Const(EvalConst::String(method)),
                    ));
                }
                let args = self.parse_call_args()?;
                Ok(EvalExpr::DynamicStaticMethodCall {
                    class_name: Box::new(class_name),
                    method: Box::new(EvalExpr::Const(EvalConst::String(method))),
                    args,
                })
            }
            TokenKind::LBrace => {
                self.advance();
                let member = self.parse_expr()?;
                self.expect(TokenKind::RBrace)?;
                if matches!(self.current(), TokenKind::LParen) {
                    if self.consume_first_class_callable_marker() {
                        return Ok(Self::dynamic_static_method_callable_expr(class_name, member));
                    }
                    let args = self.parse_call_args()?;
                    return Ok(EvalExpr::DynamicStaticMethodCall {
                        class_name: Box::new(class_name),
                        method: Box::new(member),
                        args,
                    });
                }
                Ok(EvalExpr::DynamicClassConstantNameFetch {
                    class_name: Box::new(class_name),
                    constant: Box::new(member),
                })
            }
            TokenKind::Ident(constant) => {
                let constant = constant.clone();
                self.advance();
                Ok(EvalExpr::DynamicClassConstantFetch {
                    class_name: Box::new(class_name),
                    constant,
                })
            }
            _ => Err(EvalParseError::UnsupportedConstruct),
        }
    }

    /// Parses `new ClassName(...)` and anonymous `new class {}` expressions in eval fragments.
    pub(in crate::parser) fn parse_new_object_expr(&mut self) -> Result<EvalExpr, EvalParseError> {
        self.advance();
        let is_readonly_anonymous = matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "readonly"))
            && matches!(self.peek(), TokenKind::Ident(name) if ident_eq(name, "class"));
        if is_readonly_anonymous {
            self.advance();
        }
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "class")) {
            return self.parse_anonymous_class_expr(is_readonly_anonymous);
        }
        if let TokenKind::DollarIdent(name) = self.current() {
            let class_name = EvalExpr::LoadVar(name.clone());
            self.advance();
            let args = self.parse_optional_constructor_args()?;
            return Ok(EvalExpr::DynamicNewObject {
                class_name: Box::new(class_name),
                args,
            });
        }
        if self.consume(TokenKind::LParen) {
            let class_name = self.parse_expr()?;
            self.expect(TokenKind::RParen)?;
            let args = self.parse_optional_constructor_args()?;
            return Ok(EvalExpr::DynamicNewObject {
                class_name: Box::new(class_name),
                args,
            });
        }
        let class_name = self.parse_class_reference_name(true)?;
        let class_name = self.resolve_static_class_name(class_name);
        let args = self.parse_optional_constructor_args()?;
        Ok(EvalExpr::NewObject { class_name, args })
    }

    /// Parses an optional constructor argument list after `new` class targets.
    pub(super) fn parse_optional_constructor_args(&mut self) -> Result<Vec<EvalCallArg>, EvalParseError> {
        if matches!(self.current(), TokenKind::LParen) {
            self.parse_call_args()
        } else {
            Ok(Vec::new())
        }
    }

    /// Parses a simple or explicitly qualified PHP name.
    pub(in crate::parser) fn parse_qualified_name(&mut self) -> Result<ParsedQualifiedName, EvalParseError> {
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

    /// Parses a class-like reference name while rejecting PHP-reserved unqualified names.
    pub(in crate::parser) fn parse_class_reference_name(
        &mut self,
        allow_relative_keywords: bool,
    ) -> Result<ParsedQualifiedName, EvalParseError> {
        let name = self.parse_qualified_name()?;
        if self.class_reference_name_is_reserved(&name, allow_relative_keywords) {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        Ok(name)
    }

    /// Returns whether a parsed class-like reference uses a PHP-reserved bare name.
    pub(in crate::parser) fn class_reference_name_is_reserved(
        &self,
        name: &ParsedQualifiedName,
        allow_relative_keywords: bool,
    ) -> bool {
        if name.absolute || name.name.contains('\\') {
            return false;
        }
        if allow_relative_keywords
            && ["self", "parent", "static"]
                .iter()
                .any(|keyword| ident_eq(&name.name, keyword))
        {
            return false;
        }
        is_reserved_class_like_name(&name.name)
    }

    /// Resolves a class name used before `::`, preserving PHP relative class keywords.
    pub(in crate::parser) fn resolve_static_class_name(&self, name: ParsedQualifiedName) -> String {
        if !name.absolute
            && ["self", "parent", "static"]
                .iter()
                .any(|keyword| ident_eq(&name.name, keyword))
        {
            return name.name.to_ascii_lowercase();
        }
        self.resolve_class_name(name)
    }

    /// Builds a call expression, adding namespace fallback for unqualified names.
    pub(in crate::parser) fn call_expr(&self, name: String, args: Vec<EvalCallArg>) -> EvalExpr {
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
    pub(in crate::parser) fn const_fetch_expr(&self, name: String) -> EvalExpr {
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
    pub(in crate::parser) fn qualify_name_in_current_namespace(&self, name: &str) -> String {
        if self.namespace.is_empty() {
            name.to_string()
        } else {
            format!("{}\\{}", self.namespace, name)
        }
    }

    /// Resolves a class name through active imports before namespace qualification.
    pub(in crate::parser) fn resolve_class_name(&self, name: ParsedQualifiedName) -> String {
        if name.absolute {
            return name.name;
        }
        if let Some(imported) = self.imports.resolve_class(&name.name) {
            return imported;
        }
        self.resolve_qualified_name(name)
    }

    /// Resolves a parsed PHP name according to the current namespace.
    pub(in crate::parser) fn resolve_qualified_name(&self, name: ParsedQualifiedName) -> String {
        if name.absolute || self.namespace.is_empty() {
            name.name
        } else {
            self.qualify_name_in_current_namespace(&name.name)
        }
    }

}
