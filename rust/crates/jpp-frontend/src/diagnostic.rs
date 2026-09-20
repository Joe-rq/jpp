use crate::ast::Span;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub message: String,
    pub span: Span,
}

impl Diagnostic {
    pub fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
        }
    }

    pub fn render(&self, filename: &str, source: &str) -> String {
        let start = self.span.start.min(source.len());
        let line_start = source[..start].rfind('\n').map_or(0, |p| p + 1);
        let line_end = source[start..]
            .find('\n')
            .map_or(source.len(), |p| start + p);
        let line = source[..start].bytes().filter(|b| *b == b'\n').count() + 1;
        let col = source[line_start..start].chars().count() + 1;
        let end = self.span.end.min(line_end).max(start);
        let width = source[start..end].chars().count().max(1);
        format!(
            "{filename}:{line}:{col}: {}\n{}\n{}{}",
            self.message,
            &source[line_start..line_end],
            " ".repeat(col - 1),
            "^".repeat(width)
        )
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} at bytes {}..{}",
            self.message, self.span.start, self.span.end
        )
    }
}
impl std::error::Error for Diagnostic {}
