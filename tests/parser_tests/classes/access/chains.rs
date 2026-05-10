//! Purpose:
//! Integration or regression tests for parser AST coverage of class chains, including deep property assign after array access, deep property array assign after array access, and deep property array push after array access.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets are parsed and assertions inspect AST shape, precedence, or expected parse failures.

use super::*;

#[test]
fn test_parse_deep_property_assign_after_array_access() {
    let stmts = parse_source("<?php $catalog->palette->colors[$i]->r = 12;");
    match &stmts[0].kind {
        StmtKind::PropertyAssign {
            object,
            property,
            value,
        } => {
            assert_eq!(property, "r");
            assert!(matches!(value.kind, ExprKind::IntLiteral(12)));
            match &object.kind {
                ExprKind::ArrayAccess { array, index } => {
                    assert!(matches!(index.kind, ExprKind::Variable(ref name) if name == "i"));
                    match &array.kind {
                        ExprKind::PropertyAccess { object, property } => {
                            assert_eq!(property, "colors");
                            match &object.kind {
                                ExprKind::PropertyAccess { object, property } => {
                                    assert_eq!(property, "palette");
                                    assert!(
                                        matches!(object.kind, ExprKind::Variable(ref name) if name == "catalog")
                                    );
                                }
                                other => {
                                    panic!("Expected nested PropertyAccess, got {:?}", other)
                                }
                            }
                        }
                        other => panic!("Expected PropertyAccess, got {:?}", other),
                    }
                }
                other => panic!("Expected ArrayAccess, got {:?}", other),
            }
        }
        other => panic!("Expected PropertyAssign, got {:?}", other),
    }
}

#[test]
fn test_parse_deep_property_array_assign_after_array_access() {
    let stmts = parse_source("<?php $catalog->palette->colors[$i]->shades[1] = 12;");
    match &stmts[0].kind {
        StmtKind::PropertyArrayAssign {
            object,
            property,
            index,
            value,
        } => {
            assert_eq!(property, "shades");
            assert!(matches!(index.kind, ExprKind::IntLiteral(1)));
            assert!(matches!(value.kind, ExprKind::IntLiteral(12)));
            match &object.kind {
                ExprKind::ArrayAccess { array, index } => {
                    assert!(matches!(index.kind, ExprKind::Variable(ref name) if name == "i"));
                    match &array.kind {
                        ExprKind::PropertyAccess { object, property } => {
                            assert_eq!(property, "colors");
                            match &object.kind {
                                ExprKind::PropertyAccess { object, property } => {
                                    assert_eq!(property, "palette");
                                    assert!(
                                        matches!(object.kind, ExprKind::Variable(ref name) if name == "catalog")
                                    );
                                }
                                other => {
                                    panic!("Expected nested PropertyAccess, got {:?}", other)
                                }
                            }
                        }
                        other => panic!("Expected PropertyAccess, got {:?}", other),
                    }
                }
                other => panic!("Expected ArrayAccess, got {:?}", other),
            }
        }
        other => panic!("Expected PropertyArrayAssign, got {:?}", other),
    }
}

#[test]
fn test_parse_deep_property_array_push_after_array_access() {
    let stmts = parse_source("<?php $catalog->palette->colors[$i]->shades[] = 12;");
    match &stmts[0].kind {
        StmtKind::PropertyArrayPush {
            object,
            property,
            value,
        } => {
            assert_eq!(property, "shades");
            assert!(matches!(value.kind, ExprKind::IntLiteral(12)));
            match &object.kind {
                ExprKind::ArrayAccess { array, index } => {
                    assert!(matches!(index.kind, ExprKind::Variable(ref name) if name == "i"));
                    match &array.kind {
                        ExprKind::PropertyAccess { object, property } => {
                            assert_eq!(property, "colors");
                            match &object.kind {
                                ExprKind::PropertyAccess { object, property } => {
                                    assert_eq!(property, "palette");
                                    assert!(
                                        matches!(object.kind, ExprKind::Variable(ref name) if name == "catalog")
                                    );
                                }
                                other => {
                                    panic!("Expected nested PropertyAccess, got {:?}", other)
                                }
                            }
                        }
                        other => panic!("Expected PropertyAccess, got {:?}", other),
                    }
                }
                other => panic!("Expected ArrayAccess, got {:?}", other),
            }
        }
        other => panic!("Expected PropertyArrayPush, got {:?}", other),
    }
}

#[test]
fn test_parse_chained_access() {
    let stmts = parse_source("<?php echo $obj->make()->prop;");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::PropertyAccess { object, property } => {
                assert_eq!(property, "prop");
                match &object.kind {
                    ExprKind::MethodCall {
                        object,
                        method,
                        args,
                    } => {
                        assert_eq!(method, "make");
                        assert!(args.is_empty());
                        assert!(matches!(object.kind, ExprKind::Variable(_)));
                    }
                    _ => panic!("Expected MethodCall inside chained access"),
                }
            }
            _ => panic!("Expected PropertyAccess"),
        },
        _ => panic!("Expected Echo"),
    }
}

#[test]
fn test_parse_property_access_after_array_index() {
    let stmts = parse_source("<?php echo $items[0]->name;");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::PropertyAccess { object, property } => {
                assert_eq!(property, "name");
                match &object.kind {
                    ExprKind::ArrayAccess { array, index } => {
                        assert!(matches!(array.kind, ExprKind::Variable(_)));
                        assert!(matches!(index.kind, ExprKind::IntLiteral(0)));
                    }
                    other => panic!("Expected ArrayAccess, got {:?}", other),
                }
            }
            other => panic!("Expected PropertyAccess, got {:?}", other),
        },
        other => panic!("Expected Echo, got {:?}", other),
    }
}

#[test]
fn test_parse_deep_mixed_property_and_array_chain() {
    let stmts = parse_source("<?php echo $catalog->palette->colors[$i]->r;");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::PropertyAccess { object, property } => {
                assert_eq!(property, "r");
                match &object.kind {
                    ExprKind::ArrayAccess { array, index } => {
                        assert!(matches!(index.kind, ExprKind::Variable(ref name) if name == "i"));
                        match &array.kind {
                            ExprKind::PropertyAccess { object, property } => {
                                assert_eq!(property, "colors");
                                match &object.kind {
                                    ExprKind::PropertyAccess { object, property } => {
                                        assert_eq!(property, "palette");
                                        assert!(matches!(object.kind, ExprKind::Variable(ref name) if name == "catalog"));
                                    }
                                    other => {
                                        panic!("Expected nested PropertyAccess, got {:?}", other)
                                    }
                                }
                            }
                            other => panic!("Expected PropertyAccess, got {:?}", other),
                        }
                    }
                    other => panic!("Expected ArrayAccess, got {:?}", other),
                }
            }
            other => panic!("Expected PropertyAccess, got {:?}", other),
        },
        other => panic!("Expected Echo, got {:?}", other),
    }
}

#[test]
fn test_parse_property_access_after_array_access_on_method_call_result() {
    let stmts = parse_source("<?php echo $shop->getItems()[0]->name;");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::PropertyAccess { object, property } => {
                assert_eq!(property, "name");
                match &object.kind {
                    ExprKind::ArrayAccess { array, index } => {
                        assert!(matches!(index.kind, ExprKind::IntLiteral(0)));
                        match &array.kind {
                            ExprKind::MethodCall {
                                object,
                                method,
                                args,
                            } => {
                                assert_eq!(method, "getItems");
                                assert!(args.is_empty());
                                assert!(
                                    matches!(object.kind, ExprKind::Variable(ref name) if name == "shop")
                                );
                            }
                            other => panic!("Expected MethodCall, got {:?}", other),
                        }
                    }
                    other => panic!("Expected ArrayAccess, got {:?}", other),
                }
            }
            other => panic!("Expected PropertyAccess, got {:?}", other),
        },
        other => panic!("Expected Echo, got {:?}", other),
    }
}
