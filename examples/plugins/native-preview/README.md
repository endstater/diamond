# Native preview protocol fixture

Install this folder from Diamond's Plugins panel, select the installed plugin,
review its command/capabilities and launch it. Open a UTF-8 file ending in
`.previewdemo` (one line per visual stripe), then select “Native preview demo”.
The host owns the editable source and autosave; the plugin returns one PNG page
and Unicode scalar anchors. Clicking a stripe requests its source line.

This intentionally draws stripes rather than compiling a document. It is an
integration fixture for plugin authors, not a PDF viewer or Typst compiler.
It uses Python's standard library only and never reads the supplied file path.
