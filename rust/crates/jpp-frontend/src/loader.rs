//! Resolve leading relative imports into one program for the existing core.
//! No second interpreter: dependency declarations precede the entry file.
use crate::{
    Diagnostic,
    ast::{Program, Span, Statement},
    lexer::{self, Kind},
    parser,
};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

pub struct SourceFile {
    pub path: PathBuf,
    pub text: String,
    pub offset: usize,
}

pub struct LoadedProgram {
    pub program: Program,
    pub sources: Vec<SourceFile>,
}

impl LoadedProgram {
    pub fn render(&self, diagnostic: &Diagnostic) -> String {
        render(&self.sources, diagnostic)
    }
}

fn render(sources: &[SourceFile], d: &Diagnostic) -> String {
    if let Some(file) = sources
        .iter()
        .find(|f| d.span.start >= f.offset && d.span.start <= f.offset + f.text.len())
    {
        Diagnostic::new(
            d.message.clone(),
            Span {
                start: d.span.start - file.offset,
                end: d.span.end.saturating_sub(file.offset).min(file.text.len()),
            },
        )
        .render(&file.path.to_string_lossy(), &file.text)
    } else {
        format!("{} (source offset {})", d.message, d.span.start)
    }
}

pub fn load(path: &Path) -> Result<LoadedProgram, String> {
    let mut loader = Loader::default();
    let mut program = loader.visit(path, true)?.expect("entry is loaded once");
    loader.statements.append(&mut program.body.statements);
    program.body.statements = loader.statements;
    Ok(LoadedProgram {
        program,
        sources: loader.sources,
    })
}

#[derive(Default)]
struct Loader {
    sources: Vec<SourceFile>,
    active: Vec<PathBuf>,
    done: HashSet<PathBuf>,
    names: HashMap<String, PathBuf>,
    statements: Vec<Statement>,
    offset: usize,
}

impl Loader {
    fn visit(&mut self, path: &Path, entry: bool) -> Result<Option<Program>, String> {
        let path = fs::canonicalize(path).map_err(|e| format!("{}: {e}", path.display()))?;
        if self.active.contains(&path) {
            return Err(format!(
                "import cycle: {} -> {}",
                self.active
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(" -> "),
                path.display()
            ));
        }
        if self.done.contains(&path) {
            return Ok(None);
        }
        let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut tokens = lexer::lex(&text).map_err(|e| e.render(&path.to_string_lossy(), &text))?;
        let offset = self.offset;
        self.offset += text.len() + 1;
        self.sources.push(SourceFile {
            path: path.clone(),
            text,
            offset,
        });
        for t in &mut tokens {
            t.span.start += offset;
            t.span.end += offset;
        }
        let mut imports = Vec::new();
        let mut cursor = 0;
        while matches!(&tokens[cursor].kind, Kind::Name(n) if n == "import") {
            let span = tokens[cursor].span;
            let imported = match tokens.get(cursor + 1).map(|t| &t.kind) {
                Some(Kind::Text(s)) if !s.is_empty() => s.clone(),
                _ => {
                    return Err(render(
                        &self.sources,
                        &Diagnostic::new("import requires a quoted relative file path", span),
                    ));
                }
            };
            if !matches!(tokens.get(cursor + 2).map(|t| &t.kind), Some(Kind::Symbol(s)) if s == ";")
            {
                return Err(render(
                    &self.sources,
                    &Diagnostic::new("expected ';' after import", span),
                ));
            }
            if Path::new(&imported).is_absolute() {
                return Err(render(
                    &self.sources,
                    &Diagnostic::new("use a relative import path", span),
                ));
            }
            imports.push((imported, span));
            cursor += 3;
        }
        let mut parsed = parser::parse_tokens(tokens.split_off(cursor))
            .map_err(|e| render(&self.sources, &e))?;
        if !entry
            && (parsed.budget.is_some()
                || parsed.body.result.is_some()
                || parsed
                    .body
                    .statements
                    .iter()
                    .any(|s| matches!(s, Statement::Expression(_))))
        {
            return Err(render(
                &self.sources,
                &Diagnostic::new(
                    "a library contains declarations; put the budget and result in the entry file",
                    parsed.body.span,
                ),
            ));
        }
        self.active.push(path.clone());
        for (relative, span) in imports {
            self.visit(&path.parent().unwrap().join(relative), false)
                .map_err(|e| {
                    format!(
                        "{}\n{e}",
                        render(
                            &self.sources,
                            &Diagnostic::new("while resolving this import", span)
                        )
                    )
                })?;
        }
        self.active.pop();
        for statement in &parsed.body.statements {
            let (name, span) = match statement {
                Statement::Let { name, span, .. } | Statement::Function { name, span, .. } => {
                    (name, *span)
                }
                _ => continue,
            };
            if let Some(previous) = self.names.get(name) {
                if previous != &path {
                    return Err(render(
                        &self.sources,
                        &Diagnostic::new(
                            format!(
                                "imported name '{name}' already declared in {}",
                                previous.display()
                            ),
                            span,
                        ),
                    ));
                }
            }
            self.names.insert(name.clone(), path.clone());
        }
        self.done.insert(path);
        if entry {
            Ok(Some(parsed))
        } else {
            self.statements.append(&mut parsed.body.statements);
            Ok(None)
        }
    }
}
