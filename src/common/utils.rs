use std::sync::Arc;

use pest::Span;
use self_cell::self_cell;
use tower_lsp::lsp_types::{Position, Range};

#[derive(Clone, Debug)]
pub struct LineIndex<'i> {
    input: &'i str,
    lines: Vec<&'i str>,
}

impl<'i> LineIndex<'i> {
    pub fn new(input: &'i str) -> Self {
        let mut lines: Vec<&str> = input.split_inclusive('\n').collect();
        if input.is_empty() {
            lines.push(input);
        }
        if input.ends_with('\n') {
            lines.push(&input[input.len()..]);
        }
        Self { input, lines }
    }

    fn str_offset(&self, s: &str) -> usize {
        unsafe { s.as_ptr().offset_from(self.input.as_ptr()) as usize }
    }

    pub fn position(&self, offset: usize) -> Position {
        let index = self
            .lines
            .binary_search_by_key(&offset, |line| self.str_offset(line))
            .unwrap_or_else(|index| index - 1);
        let line = self.lines[index];
        let bytes = offset - self.str_offset(line);
        let character = line
            .get(..bytes)
            .map(|s| s.encode_utf16().count())
            .unwrap_or(0);
        Position {
            line: index as u32,
            character: character as u32,
        }
    }

    pub fn range(&self, span: Span) -> Range {
        Range {
            start: self.position(span.start()),
            end: self.position(span.end()),
        }
    }

    pub fn offset(&self, position: Position) -> Option<usize> {
        let line = self.lines.get(position.line as usize)?;
        let mut character = 0;
        for (i, ch) in line.char_indices() {
            if character >= position.character as usize {
                return Some(self.str_offset(line) + i);
            }
            let mut buf = [0; 2];
            character += ch.encode_utf16(&mut buf).len();
        }
        if character >= position.character as usize {
            Some(self.str_offset(line) + line.len())
        } else {
            None
        }
    }
}

self_cell!(
    struct LineIndexCell {
        owner: Arc<String>,
        #[covariant]
        dependent: LineIndex,
    }
    impl {Debug}
);

#[derive(Clone, Debug)]
pub struct OwnedLineIndex(Arc<LineIndexCell>);

impl OwnedLineIndex {
    pub fn new(input: Arc<String>) -> Self {
        Self(Arc::new(LineIndexCell::new(input, |input| {
            LineIndex::new(input.as_str())
        })))
    }

    pub fn position(&self, offset: usize) -> Position {
        self.0.borrow_dependent().position(offset)
    }

    pub fn range(&self, span: Span) -> Range {
        self.0.borrow_dependent().range(span)
    }

    pub fn offset(&self, position: Position) -> Option<usize> {
        self.0.borrow_dependent().offset(position)
    }
}
