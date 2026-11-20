use std::{
    fmt::Write as _,
    io::{self, Write as _},
    path::{Path, PathBuf},
};

use pulldown_cmark::{Event, Parser, Tag};
use serde::Serialize;
use typst_as_lib::TypstEngine;

use crate::{manuscript::Manuscript, passage::SceneText};

const MAX_MARKDOWN_DEPTH: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Markdown,
    Pdf,
}

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("Die Markdown-Struktur ist für den PDF-Export zu tief verschachtelt.")]
    MarkdownDepth,
    #[error("PDF-Erstellung fehlgeschlagen: {0}")]
    Pdf(String),
}

pub fn export(book: &Manuscript, format: ExportFormat, path: &Path) -> Result<(), ExportError> {
    let output = match format {
        ExportFormat::Markdown => markdown(book).into_bytes(),
        ExportFormat::Pdf => pdf(book)?,
    };
    let directory = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let failed = |source| ExportError::Io {
        path: path.into(),
        source,
    };
    let mut temporary = tempfile::NamedTempFile::new_in(directory).map_err(failed)?;
    temporary.write_all(&output).map_err(failed)?;
    temporary.as_file().sync_all().map_err(failed)?;
    temporary
        .persist(path)
        .map_err(|error| failed(error.error))?;
    Ok(())
}

pub fn preview_markdown(scene: &SceneText) -> String {
    let source = scene.text();
    let mut output = String::new();
    let mut end = 0;
    for concept in scene.concepts() {
        let before = &source[end..concept.range.start];
        output.push_str(before);
        let marked = &source[concept.range.clone()];
        let starts_line = source[..concept.range.start].is_empty()
            || source[..concept.range.start].ends_with('\n');
        let ends_line = source[concept.range.end..].is_empty()
            || source[concept.range.end..].starts_with(['\n', '\r'])
            || marked.ends_with('\n');
        if starts_line && ends_line {
            if !output.is_empty() && !output.ends_with("\n\n") {
                output.push('\n');
            }
            output.push_str("> **Konzept**\n>\n");
            for line in marked.lines() {
                let _ = writeln!(output, "> {line}");
            }
            output.push('\n');
        } else {
            output.push_str(" **[Konzept]** ");
            output.push_str(marked);
            output.push_str(" **[/Konzept]** ");
        }
        end = concept.range.end;
    }
    output.push_str(&source[end..]);
    output
}

fn markdown(book: &Manuscript) -> String {
    let mut output = format!("# {}\n\n", escape_title(book.title()));
    for chapter in book.chapters() {
        let _ = writeln!(output, "## {}\n", escape_title(&chapter.title));
        for id in &chapter.scenes {
            if let Some(scene) = book.scene(*id) {
                let text = preview_markdown(&scene.text);
                output.push_str(text.trim_end_matches(['\r', '\n']));
                output.push_str("\n\n");
            }
        }
    }
    output
}

fn escape_title(title: &str) -> String {
    let mut escaped = String::new();
    for character in title.trim().chars() {
        if character.is_ascii_punctuation() {
            escaped.push('\\');
        }
        let character = if character.is_control() {
            ' '
        } else {
            character
        };
        escaped.push(character);
    }
    escaped
}

#[derive(Serialize)]
struct DocumentTree<'a> {
    title: &'a str,
    chapters: Vec<ChapterTree<'a>>,
}

#[derive(Serialize)]
struct ChapterTree<'a> {
    title: &'a str,
    scenes: Vec<Vec<Node>>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
enum Node {
    Text(String),
    Code(String),
    LineBreak,
    Rule,
    Container { style: Style, children: Vec<Node> },
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case", tag = "name", content = "parameter")]
enum Style {
    Paragraph,
    Heading(u8),
    Bullets,
    Numbered(u64),
    Quote,
    CodeBlock,
    Bold,
    Italic,
    Plain,
}

impl Style {
    fn from_tag(tag: Tag<'_>) -> Self {
        match tag {
            Tag::Paragraph => Self::Paragraph,
            Tag::Heading { level, .. } => Self::Heading(level as u8 + 1),
            Tag::List(None) => Self::Bullets,
            Tag::List(Some(start)) => Self::Numbered(start),
            Tag::BlockQuote(_) => Self::Quote,
            Tag::CodeBlock(_) => Self::CodeBlock,
            Tag::Strong => Self::Bold,
            Tag::Emphasis => Self::Italic,
            _ => Self::Plain,
        }
    }
}

fn parse_markdown(text: &str) -> Result<Vec<Node>, ExportError> {
    let mut root = Vec::new();
    let mut stack: Vec<(Style, Vec<Node>)> = Vec::new();
    for event in Parser::new(text) {
        let node = match event {
            Event::Start(tag) => {
                if stack.len() >= MAX_MARKDOWN_DEPTH {
                    return Err(ExportError::MarkdownDepth);
                }
                stack.push((Style::from_tag(tag), Vec::new()));
                continue;
            }
            Event::End(_) => {
                let Some((style, children)) = stack.pop() else {
                    continue;
                };
                Node::Container { style, children }
            }
            Event::Text(text) | Event::Html(text) | Event::InlineHtml(text) => {
                Node::Text(text.into_string())
            }
            Event::Code(text) => Node::Code(text.into_string()),
            Event::SoftBreak => Node::Text(" ".into()),
            Event::HardBreak => Node::LineBreak,
            Event::Rule => Node::Rule,
            _ => continue,
        };
        if let Some((_, children)) = stack.last_mut() {
            children.push(node);
        } else {
            root.push(node);
        }
    }
    Ok(root)
}

fn pdf(book: &Manuscript) -> Result<Vec<u8>, ExportError> {
    let chapters = book
        .chapters()
        .iter()
        .map(|chapter| {
            let scenes = chapter
                .scenes
                .iter()
                .filter_map(|id| book.scene(*id))
                .map(|scene| parse_markdown(&preview_markdown(&scene.text)))
                .collect::<Result<_, _>>()?;
            Ok(ChapterTree {
                title: &chapter.title,
                scenes,
            })
        })
        .collect::<Result<_, ExportError>>()?;
    let input = serde_json::to_vec(&DocumentTree {
        title: book.title(),
        chapters,
    })
    .map_err(|error| ExportError::Pdf(error.to_string()))?;
    let engine = TypstEngine::builder()
        .main_file(TEMPLATE)
        .with_static_file_resolver([("manuscript.json", input)])
        .fonts(typst_assets::fonts())
        .build();
    let document = engine
        .compile()
        .output
        .map_err(|error| ExportError::Pdf(error.to_string()))?;
    typst_pdf::pdf(&document, &typst_pdf::PdfOptions::default()).map_err(|errors| {
        ExportError::Pdf(
            errors
                .iter()
                .map(|error| error.message.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
        )
    })
}

const TEMPLATE: &str = r##"
#let book = json("manuscript.json")
#set document(title: book.title)
#set page("a4", margin: (x: 28mm, y: 25mm), numbering: "1")
#set text(font: "Libertinus Serif", size: 11pt, lang: "de")
#set par(spacing: 0.85em, leading: 0.62em)

#let render(node) = {
  if node.kind == "text" { text(node.value) }
  else if node.kind == "code" { raw(node.value) }
  else if node.kind == "line_break" { linebreak() }
  else if node.kind == "rule" {
    block(above: 1em, below: 1em)[#line(length: 30%, stroke: gray)]
  } else {
    let style = node.value.style
    let children = node.value.children
    let elements = children.map(render)
    let content = elements.join()
    if style.name == "paragraph" { par(content) }
    else if style.name == "heading" { heading(level: style.parameter, content) }
    else if style.name == "bullets" { list(..elements) }
    else if style.name == "numbered" { enum(start: style.parameter, ..elements) }
    else if style.name == "quote" {
      block(width: 100%, inset: (left: 1em), stroke: (left: 0.8pt + rgb("#94723f")), content)
    } else if style.name == "code_block" {
      raw(children.map(child => child.value).join(), block: true)
    } else if style.name == "bold" { strong(content) }
    else if style.name == "italic" { emph(content) }
    else { content }
  }
}

#title(book.title)
#for chapter in book.chapters {
  heading(level: 1, chapter.title)
  for scene in chapter.scenes {
    scene.map(render).join()
    parbreak()
  }
}
"##;
