use std::collections::HashMap;

use tree_sitter::Language;

pub struct LanguageEntry {
    pub name: &'static str,
    pub grammar: Language,
    pub extensions: &'static [&'static str],
    pub query_source: Option<&'static str>,
}

pub struct LanguageRegistry {
    by_name: HashMap<&'static str, LanguageEntry>,
    ext_map: HashMap<&'static str, &'static str>,
}

impl LanguageRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            by_name: HashMap::new(),
            ext_map: HashMap::new(),
        };
        registry.register_all();
        registry
    }

    pub fn detect_language(&self, path: &std::path::Path) -> Option<&'static str> {
        let ext = path.extension()?.to_str()?;
        self.ext_map.get(ext).copied()
    }

    pub fn get_parser(&self, lang: &str) -> Option<tree_sitter::Parser> {
        let entry = self.by_name.get(lang)?;
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&entry.grammar).ok()?;
        Some(parser)
    }

    pub fn get_query(&self, lang: &str) -> Option<tree_sitter::Query> {
        let entry = self.by_name.get(lang)?;
        let source = entry.query_source?;
        // Queries may fail to compile for some grammars if node types differ;
        // we treat that as "no query available" and fall back to generic parsing.
        tree_sitter::Query::new(&entry.grammar, source).ok()
    }

    pub fn supported_languages(&self) -> Vec<&'static str> {
        let mut langs: Vec<_> = self.by_name.keys().copied().collect();
        langs.sort();
        langs
    }

    /// Static list of language names for completions (no instance needed).
    pub fn static_language_names() -> &'static [&'static str] {
        &[
            "bash", "c", "cpp", "csharp", "css", "go", "hcl", "html", "java",
            "javascript", "json", "kotlin", "markdown", "php", "python", "ruby",
            "rust", "scala", "swift", "toml", "typescript", "yaml",
        ]
    }

    fn register(&mut self, entry: LanguageEntry) {
        for ext in entry.extensions {
            self.ext_map.insert(ext, entry.name);
        }
        self.by_name.insert(entry.name, entry);
    }

    fn register_all(&mut self) {
        self.register(LanguageEntry {
            name: "rust",
            grammar: tree_sitter_rust::LANGUAGE.into(),
            extensions: &["rs"],
            query_source: Some(include_str!("../../languages/rust.scm")),
        });

        self.register(LanguageEntry {
            name: "python",
            grammar: tree_sitter_python::LANGUAGE.into(),
            extensions: &["py", "pyi"],
            query_source: Some(include_str!("../../languages/python.scm")),
        });

        self.register(LanguageEntry {
            name: "javascript",
            grammar: tree_sitter_javascript::LANGUAGE.into(),
            extensions: &["js", "jsx", "mjs", "cjs"],
            query_source: Some(include_str!("../../languages/typescript.scm")),
        });

        self.register(LanguageEntry {
            name: "typescript",
            grammar: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            extensions: &["ts"],
            query_source: Some(include_str!("../../languages/typescript.scm")),
        });

        self.register(LanguageEntry {
            name: "tsx",
            grammar: tree_sitter_typescript::LANGUAGE_TSX.into(),
            extensions: &["tsx"],
            query_source: Some(include_str!("../../languages/typescript.scm")),
        });

        self.register(LanguageEntry {
            name: "go",
            grammar: tree_sitter_go::LANGUAGE.into(),
            extensions: &["go"],
            query_source: Some(include_str!("../../languages/go.scm")),
        });

        self.register(LanguageEntry {
            name: "java",
            grammar: tree_sitter_java::LANGUAGE.into(),
            extensions: &["java"],
            query_source: Some(include_str!("../../languages/java.scm")),
        });

        self.register(LanguageEntry {
            name: "c",
            grammar: tree_sitter_c::LANGUAGE.into(),
            extensions: &["c", "h"],
            query_source: Some(include_str!("../../languages/c.scm")),
        });

        self.register(LanguageEntry {
            name: "cpp",
            grammar: tree_sitter_cpp::LANGUAGE.into(),
            extensions: &["cpp", "cc", "cxx", "hpp", "hxx", "hh"],
            query_source: Some(include_str!("../../languages/cpp.scm")),
        });

        self.register(LanguageEntry {
            name: "csharp",
            grammar: tree_sitter_c_sharp::LANGUAGE.into(),
            extensions: &["cs"],
            query_source: Some(include_str!("../../languages/csharp.scm")),
        });

        self.register(LanguageEntry {
            name: "ruby",
            grammar: tree_sitter_ruby::LANGUAGE.into(),
            extensions: &["rb", "rake", "gemspec"],
            query_source: Some(include_str!("../../languages/ruby.scm")),
        });

        self.register(LanguageEntry {
            name: "php",
            grammar: tree_sitter_php::LANGUAGE_PHP.into(),
            extensions: &["php"],
            query_source: Some(include_str!("../../languages/php.scm")),
        });

        self.register(LanguageEntry {
            name: "swift",
            grammar: tree_sitter_swift::LANGUAGE.into(),
            extensions: &["swift"],
            query_source: Some(include_str!("../../languages/swift.scm")),
        });

        self.register(LanguageEntry {
            name: "kotlin",
            grammar: tree_sitter_kotlin_ng::LANGUAGE.into(),
            extensions: &["kt", "kts"],
            query_source: Some(include_str!("../../languages/kotlin.scm")),
        });

        self.register(LanguageEntry {
            name: "scala",
            grammar: tree_sitter_scala::LANGUAGE.into(),
            extensions: &["scala", "sc"],
            query_source: Some(include_str!("../../languages/scala.scm")),
        });

        self.register(LanguageEntry {
            name: "bash",
            grammar: tree_sitter_bash::LANGUAGE.into(),
            extensions: &["sh", "bash", "zsh"],
            query_source: Some(include_str!("../../languages/bash.scm")),
        });

        self.register(LanguageEntry {
            name: "hcl",
            grammar: tree_sitter_hcl::LANGUAGE.into(),
            extensions: &["tf", "hcl", "tfvars"],
            query_source: Some(include_str!("../../languages/hcl.scm")),
        });

        // Structured data formats — no symbol queries needed
        self.register(LanguageEntry {
            name: "json",
            grammar: tree_sitter_json::LANGUAGE.into(),
            extensions: &["json", "jsonc"],
            query_source: None,
        });

        self.register(LanguageEntry {
            name: "toml",
            grammar: tree_sitter_toml_ng::LANGUAGE.into(),
            extensions: &["toml"],
            query_source: None,
        });

        self.register(LanguageEntry {
            name: "yaml",
            grammar: tree_sitter_yaml::LANGUAGE.into(),
            extensions: &["yml", "yaml"],
            query_source: None,
        });

        self.register(LanguageEntry {
            name: "html",
            grammar: tree_sitter_html::LANGUAGE.into(),
            extensions: &["html", "htm"],
            query_source: None,
        });

        self.register(LanguageEntry {
            name: "css",
            grammar: tree_sitter_css::LANGUAGE.into(),
            extensions: &["css", "scss"],
            query_source: None,
        });

        self.register(LanguageEntry {
            name: "markdown",
            grammar: tree_sitter_md::LANGUAGE.into(),
            extensions: &["md", "markdown"],
            query_source: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_language_detection() {
        let registry = LanguageRegistry::new();

        // Test basic extensions
        assert_eq!(registry.detect_language(Path::new("main.rs")), Some("rust"));
        assert_eq!(
            registry.detect_language(Path::new("app.ts")),
            Some("typescript")
        );
        assert_eq!(
            registry.detect_language(Path::new("index.js")),
            Some("javascript")
        );
        assert_eq!(registry.detect_language(Path::new("main.go")), Some("go"));

        // Test unknown extension
        assert_eq!(registry.detect_language(Path::new("unknown.xyz")), None);
        // Test no extension
        assert_eq!(registry.detect_language(Path::new("Makefile")), None);
    }

    #[test]
    fn test_get_parser_and_query() {
        let registry = LanguageRegistry::new();

        // Test parser retrieval
        let parser = registry.get_parser("rust");
        if parser.is_none() {
            println!("Failed to get rust parser.");
        }
        assert!(parser.is_some(), "Parser for rust should exist");
        assert!(registry.get_parser("typescript").is_some());
        assert!(registry.get_parser("unknown_lang").is_none());

        // Test query retrieval
        assert!(registry.get_query("rust").is_some());

        // JSON has no query source registered, so query should be none
        assert!(registry.get_query("json").is_none());
    }
}
