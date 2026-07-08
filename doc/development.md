# DataFlex LSP Server Development

## Overview
A brief overview of the separate repos and their responsibilities, along with some major components and descriptions of how the source code is organized.

### Repos
- **dataflex-lsp** - This is the main repo containing the LSP server implementation.
- **tree-sitter-dataflex** - This contains the tree-sitter based parser grammar, which is used by the indexer and to parse editor documents.
- **vscode-dataflex repo** - This contains the VSCode extension, which mainly launches the LSP server.
- **dataflex-lsp-workspace** - An umbrella folder with a `cargo-make` script for development, testing, and building all the components with one command.

### DataFlex-LSP main modules
- **language_server** - The main LSP entry point, start from an LSP request here to follow the path of the implementation.
- **dataflex_document** - Open editor document state, directs all editor based features such as code completion etc.
- **index** - The in-memory index, index symbol structures, workspace information, indexer, query lookup tables, etc.

### DataFlex-LSP major components and their relationship
The main entry point is `DataFlexLanguageServer`, which owns the LSP server communication, the `Indexer`, and tracks open editor documents with `DataFlexDocument`.

Each open editor document is represented by `DataFlexDocument`, which holds the document content via `LineMap`, and the syntax tree.
It uses `DocumentContext` together with the syntax tree to determine the syntactic context of a specific position in a document,
which can be `ClassReference`, `MethodReference`, `DotMemberExpr` etc.
It uses `ReferenceResolver` to look up symbols in the index based on the syntactic context etc., which is used by goto definition, and to show information on mouse hover, etc.
Some editor document based functionality is further delegated to other types like `CodeCompletion`, `ScopeBalancer`, `ParameterInfo` etc.

The index is represented by `Index`, which holds all the index file data and workspace information.
Each indexed file is represented by `IndexFile`, which has a list of symbols represented by `IndexSymbol`, which in turn can be `ClassSymbol`, `MethodSymbol` etc.
Each symbol is identified by a `SymbolPath`, which is its name qualified with its parent symbol names, e.g. `cMyClass.MyMethod`.
The index is populated by `Indexer`, which takes the workspace paths and recusively indexes all relevant files.

## Open tasks / known issues
- Fuzzy matching for workspace symbols
- Support `External_Function`, `Register_Function`, in parser and indexing
- Handle `#ifdef` in parser
- Support LSP file watcher notifications to re-index externally modified files, e.g. from git pull
- Handle renamed files, added/removed files, updating index appropriately
- Handle non-package manager libraries, indexing additional paths
- Avoid re-indexing all files after opening workspace and loading index
- Support project/file-dependency aware index lookups
- Support separate toolchains for each project
- Code completion sorting/ranking, local variables before global etc.
- Collect doc comments/comments in addition to "description" for mouse hover details
- Enhance code completion with documentation and additional details
- Enhance goto definition to narrow down candidates based on object receiver class
- Show inline hint after `end` block, indicating matching `begin`
- Consider DataFlex specific UI in VSCode, e.g. project file navigator like in DF Studio
- Rename Symbol feature
- Find References feature
- Consider relevant code actions
- Diagnostic warnings, e.g. likely unreachable method call
- Expand support for additional editors, Zed, Neovim, etc.
