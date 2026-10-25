use teloxide::types::{
    InputRichBlock, InputRichBlockButtons, InputRichBlockDetails, InputRichBlockDivider,
    InputRichBlockParagraph, InputRichBlockSectionHeading, InputRichBlockTable, RichBlockTableCell,
    RichMessageButton, RichText, RichTextBold, RichTextItalic,
};

pub use crate::utils::render_progress_bar;

// ── Text Formatting Helpers ──────────────────────────────────────────────────

#[inline]
pub fn text(val: impl Into<String>) -> RichText {
    RichText::from(val.into())
}

#[inline]
pub fn bold(content: impl Into<RichText>) -> RichText {
    RichTextBold::new(content).into()
}

#[inline]
pub fn italic(content: impl Into<RichText>) -> RichText {
    RichTextItalic::new(content).into()
}

// ── Block Builders ───────────────────────────────────────────────────────────

#[inline]
pub fn heading(title: impl Into<RichText>, size: u8) -> InputRichBlock {
    InputRichBlock::Heading(InputRichBlockSectionHeading {
        text: title.into(),
        size,
    })
}

#[inline]
pub fn divider() -> InputRichBlock {
    InputRichBlock::Divider(InputRichBlockDivider {})
}

#[inline]
pub fn paragraph(content: impl Into<RichText>) -> InputRichBlock {
    InputRichBlock::Paragraph(InputRichBlockParagraph {
        text: content.into(),
    })
}

#[inline]
pub fn details(
    summary: impl Into<RichText>,
    blocks: Vec<InputRichBlock>,
    is_open: bool,
) -> InputRichBlock {
    InputRichBlock::Details(InputRichBlockDetails {
        summary: summary.into(),
        blocks,
        is_open: Some(is_open),
    })
}

#[inline]
pub fn buttons(buttons: Vec<RichMessageButton>, align: &str) -> InputRichBlock {
    InputRichBlock::Buttons(InputRichBlockButtons::new(buttons).align(align))
}

// ── Table Cells & Fluent Cell Builder ────────────────────────────────────────

#[inline]
pub fn table_cell(
    content: impl Into<RichText>,
    is_header: bool,
    align: &str,
) -> RichBlockTableCell {
    RichBlockTableCell {
        text: Some(content.into()),
        is_header: Some(is_header),
        colspan: Some(1),
        rowspan: Some(1),
        align: align.to_string(),
        valign: "middle".to_string(),
    }
}

#[inline]
pub fn cell_left(content: impl Into<RichText>) -> RichBlockTableCell {
    table_cell(content, false, "left")
}

#[inline]
pub fn cell_right(content: impl Into<RichText>) -> RichBlockTableCell {
    table_cell(content, false, "right")
}

#[inline]
pub fn cell_center(content: impl Into<RichText>) -> RichBlockTableCell {
    table_cell(content, false, "center")
}

#[inline]
pub fn cell_header(content: impl Into<RichText>, align: &str) -> RichBlockTableCell {
    table_cell(content, true, align)
}

#[derive(Debug, Clone)]
pub struct RichCell {
    inner: RichBlockTableCell,
}

impl RichCell {
    pub fn new(content: impl Into<RichText>) -> Self {
        Self {
            inner: table_cell(content, false, "left"),
        }
    }

    pub fn left(mut self) -> Self {
        self.inner.align = "left".to_string();
        self
    }

    pub fn right(mut self) -> Self {
        self.inner.align = "right".to_string();
        self
    }

    pub fn center(mut self) -> Self {
        self.inner.align = "center".to_string();
        self
    }

    pub fn header(mut self, is_header: bool) -> Self {
        self.inner.is_header = Some(is_header);
        self
    }

    pub fn colspan(mut self, span: u32) -> Self {
        self.inner.colspan = Some(span);
        self
    }

    pub fn build(self) -> RichBlockTableCell {
        self.inner
    }
}

impl From<RichCell> for RichBlockTableCell {
    fn from(cell: RichCell) -> Self {
        cell.inner
    }
}

// ── Fluent Table Builder ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RichTableBuilder {
    rows: Vec<Vec<RichBlockTableCell>>,
    is_bordered: bool,
    is_striped: bool,
    is_compact: bool,
    caption: Option<RichText>,
}

impl Default for RichTableBuilder {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            is_bordered: false,
            is_striped: true,
            is_compact: true,
            caption: None,
        }
    }
}

impl RichTableBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bordered(mut self, bordered: bool) -> Self {
        self.is_bordered = bordered;
        self
    }

    pub fn striped(mut self, striped: bool) -> Self {
        self.is_striped = striped;
        self
    }

    pub fn compact(mut self, compact: bool) -> Self {
        self.is_compact = compact;
        self
    }

    pub fn caption(mut self, caption: impl Into<RichText>) -> Self {
        self.caption = Some(caption.into());
        self
    }

    pub fn row(mut self, cells: impl IntoIterator<Item = impl Into<RichBlockTableCell>>) -> Self {
        self.rows.push(cells.into_iter().map(Into::into).collect());
        self
    }

    pub fn kv_row(self, label: impl Into<RichText>, value: impl Into<RichText>) -> Self {
        self.row(vec![cell_left(label), cell_right(value)])
    }

    pub fn build(self) -> InputRichBlockTable {
        InputRichBlockTable {
            cells: self.rows,
            is_bordered: Some(self.is_bordered),
            is_striped: Some(self.is_striped),
            is_compact: Some(self.is_compact),
            caption: self.caption,
        }
    }

    pub fn build_block(self) -> InputRichBlock {
        InputRichBlock::Table(self.build())
    }
}
