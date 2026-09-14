// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::LanguageId;

pub(crate) fn highlight_query_for(language: LanguageId) -> &'static str {
    match language {
        LanguageId::Bash => BASH_HIGHLIGHTS_QUERY,
        LanguageId::Fish => tree_sitter_fish::HIGHLIGHTS_QUERY,
        LanguageId::Powershell => tree_sitter_powershell::HIGHLIGHTS_QUERY,
        LanguageId::Zsh => tree_sitter_zsh::HIGHLIGHT_QUERY,
    }
}

// `tree-sitter-bash` ships a query file but does not export it from the Rust
// crate. Keep a deliberately small OxideTerm-local query so common remote
// shell files get real tree-sitter spans instead of falling back to plain text.
const BASH_HIGHLIGHTS_QUERY: &str = r#"
[
  "if"
  "then"
  "else"
  "elif"
  "fi"
  "for"
  "while"
  "do"
  "done"
  "case"
  "esac"
  "function"
  "in"
] @keyword
(comment) @comment
(string) @string
(raw_string) @string
(command_name) @function
(variable_name) @variable

; Keep structural shell punctuation visible without treating dashes in words as operators.
[
  "$"
  "&&"
  "||"
  "&"
  "|"
  ";"
  ";;"
  ">"
  ">>"
  "<"
  "<<"
  "<<<"
  "="
  "=="
  "=~"
  "+"
  "-"
  "*"
  "/"
  "%"
] @operator
"#;

// `tree-sitter-cmake` ships a highlight query file but does not export it from
// the Rust crate. Keep a compact local query for the scopes our editor theme
// already maps instead of reaching into Cargo's private registry layout.
#[allow(dead_code)]
const CMAKE_HIGHLIGHTS_QUERY: &str = r#"
[
  (function)
  (endfunction)
  (macro)
  (endmacro)
  (if)
  (elseif)
  (else)
  (endif)
  (foreach)
  (endforeach)
  (while)
  (endwhile)
] @keyword

(normal_command (identifier) @function)
(quoted_argument) @string
(bracket_argument) @string
(line_comment) @comment
(bracket_comment) @comment
(variable) @variable
"#;

// The Common Lisp crate intentionally leaves query exports disabled. This
// compact query gives Lisp files useful editor color without tying us to the
// crate's source layout.
#[allow(dead_code)]
const LISP_HIGHLIGHTS_QUERY: &str = r#"
(comment) @comment
(str_lit) @string
(num_lit) @number
(sym_lit) @variable
((sym_lit) @keyword
  (#match? @keyword "^(defun|defmacro|lambda|let|let\\*|if|cond|loop|return)$"))
"#;

// `tree-sitter-javascript` also ships query files without exporting them from
// the crate. This local query intentionally covers the common scopes used by
// the editor color mapper while staying small enough to keep compile failures
// obvious when the grammar changes.
#[allow(dead_code)]
const JAVASCRIPT_HIGHLIGHTS_QUERY: &str = r#"
[
  "async"
  "await"
  "break"
  "case"
  "catch"
  "class"
  "const"
  "continue"
  "debugger"
  "default"
  "delete"
  "do"
  "else"
  "export"
  "extends"
  "finally"
  "for"
  "from"
  "function"
  "if"
  "import"
  "in"
  "instanceof"
  "let"
  "new"
  "of"
  "return"
  "switch"
  "throw"
  "try"
  "typeof"
  "var"
  "void"
  "while"
  "with"
  "yield"
] @keyword
(comment) @comment
(string) @string
(template_string) @string
(number) @number
(identifier) @variable
(property_identifier) @property
(function_declaration name: (identifier) @function)
(method_definition name: (property_identifier) @function)
(pair key: (property_identifier) @property)
"#;
