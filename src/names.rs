#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NameKind {
    Unqualified,
    Qualified,
    FullyQualified,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Name {
    pub kind: NameKind,
    pub parts: Vec<String>,
    text: String,
}

impl Name {
    pub fn unqualified(name: impl Into<String>) -> Self {
        Self {
            kind: NameKind::Unqualified,
            parts: vec![name.into()],
            text: String::new(),
        }
        .with_text()
    }

    pub fn qualified(parts: Vec<String>) -> Self {
        let kind = if parts.len() <= 1 {
            NameKind::Unqualified
        } else {
            NameKind::Qualified
        };
        Self {
            kind,
            parts,
            text: String::new(),
        }
        .with_text()
    }

    pub fn from_parts(kind: NameKind, parts: Vec<String>) -> Self {
        let kind = if parts.len() <= 1 && kind == NameKind::Qualified {
            NameKind::Unqualified
        } else {
            kind
        };
        Self {
            kind,
            parts,
            text: String::new(),
        }
        .with_text()
    }

    fn with_text(mut self) -> Self {
        self.text = self.parts.join("\\");
        self
    }

    pub fn as_canonical(&self) -> String {
        self.text.clone()
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    pub fn is_unqualified(&self) -> bool {
        self.kind == NameKind::Unqualified
    }

    pub fn is_fully_qualified(&self) -> bool {
        self.kind == NameKind::FullyQualified
    }

    pub fn last_segment(&self) -> Option<&str> {
        self.parts.last().map(String::as_str)
    }
}

impl std::fmt::Display for Name {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::ops::Deref for Name {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl PartialEq<str> for Name {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for Name {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<String> for Name {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other
    }
}

impl From<&str> for Name {
    fn from(value: &str) -> Self {
        Name::unqualified(value)
    }
}

impl From<String> for Name {
    fn from(value: String) -> Self {
        Name::unqualified(value)
    }
}

pub fn canonical_name_for_decl(namespace: Option<&str>, local_name: &str) -> String {
    if let Some(namespace) = namespace {
        if !namespace.is_empty() {
            return format!("{}\\{}", namespace, local_name);
        }
    }
    local_name.to_string()
}

pub fn mangle_fqn(name: &str) -> String {
    let mut mangled = String::new();
    for ch in name.chars() {
        match ch {
            'A'..='Z' | 'a'..='z' | '0'..='9' => mangled.push(ch),
            '_' => mangled.push_str("_u_"),
            '\\' => mangled.push_str("_N_"),
            _ => panic!("unsupported symbol character in mangled name: {}", ch),
        }
    }
    mangled
}

pub fn function_symbol(name: &str) -> String {
    format!("_fn_{}", mangle_fqn(name))
}

pub fn function_epilogue_symbol(name: &str) -> String {
    format!("{}_epilogue", function_symbol(name))
}

pub fn method_symbol(class_name: &str, method_name: &str) -> String {
    format!(
        "_method_{}_{}",
        mangle_fqn(class_name),
        mangle_fqn(method_name)
    )
}

pub fn static_method_symbol(class_name: &str, method_name: &str) -> String {
    format!(
        "_static_{}_{}",
        mangle_fqn(class_name),
        mangle_fqn(method_name)
    )
}

pub fn enum_case_symbol(enum_name: &str, case_name: &str) -> String {
    format!(
        "_enum_case_{}_{}",
        mangle_fqn(enum_name),
        mangle_fqn(case_name)
    )
}
