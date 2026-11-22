# diamond-document

A GUI-independent semantic Markdown model. Public edit positions are Unicode scalar indices; parser/source offsets are bytes. `Document::text()` joins the visible text of successive blocks with one newline. `Block::marks` use scalar offsets local to that block.

Supported formatted blocks are paragraphs, headings, single-paragraph quotations, flat one-line bullet/ordered/task list items (including blank separators), GFM tables, standalone images, and fenced or indented code. Horizontal rules are structural blocks with empty semantic text. Inline bold, italic, strike and code become semantic marks. Inline Markdown links and wikilinks (including aliases) retain target metadata and scalar label ranges. `toggle_task` changes only the checkbox source atom and shares source-edit undo history. Their Markdown delimiters are absent from the editable projection. Untouched blocks, their separators, CRLF and unknown syntax retain their original source bytes.

HTML, reference definitions, complex nested/multi-paragraph lists, mixed paragraphs with inline images, hard line breaks and other unrepresented constructs stay raw. Tables carry cell/column alignment metadata and scalar cell ranges; standalone images carry target, title and alt metadata. Source transactions edit these structures; legacy semantic mutations are declined. Table separators and the alt-less image label are structural projection anchors, not hidden Markdown characters. Raw blocks display their source without code styling and reject legacy semantic edits; use source mode for those blocks. Embeds remain raw. Legacy semantic mutations of link-bearing blocks are declined to avoid losing targets; the live editor safely edits their actual source transactions. Formatting may canonicalize only affected supported blocks. Code-block language identifiers survive edits. Inserted paragraph boundaries serialize with blank lines; flat lists continue their item kind.

Text, formatting and block-kind edits each form one undo transaction. Undo/redo restores source preservation metadata as well as semantic content. The bounded history retains 256 transactions. The engine is a small-document foundation, not a rope or an incremental CommonMark implementation. A mark applied to a nonempty range trims boundary whitespace; an empty selection does not create a pending typing style.

Every mutation reparses the candidate serialization and compares block text, kinds and scalar mark coverage before committing. Unsupported combinations (including crossing emphasis ranges that cannot be serialized faithfully) fail atomically with `UnrepresentableEdit`; the editor can offer source mode. Empty paragraphs are editable spacers and serialize as blank lines, which CommonMark may collapse when reopened.

Edited line-edge whitespace and whitespace at marked run boundaries may serialize as numeric entities to preserve ordinary typing without changing Markdown block structure or delimiter rules. Reference definitions omitted from the parser event stream are represented as protected raw source. Opening or parsing a document never executes content or follows remote references.

The live editor may use `replace_source_range` to type or remove real Markdown
syntax. Source positions count Unicode scalars; the operation reparses the
authoritative document and creates one undo transaction while retaining untouched
bytes exactly. `source_map` exposes per-block byte/scalar spans and before/after
semantic caret anchors. Ambiguous parser expansions are marked `is_exact=false`;
the editor displays source rather than guessing destructive offsets.

0.5 parses list lines independently: a blank or unfinished item cannot demote preceding formatted items. Bare HTTP(S) URLs become links outside existing links/code marks. Tables keep original bytes, including alignment delimiters and CRLF; table separators map to explicit source boundaries. Parsing never fetches image targets.
