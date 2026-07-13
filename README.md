# DataFlex-LSP
Language server for the DataFlex programming language.

See [features.md](doc/features.md) for current status of implementation.

## Installing
The easiest way to install and use DataFlex-LSP from Visual Studio Code is to install the pre-built binary VSCode extension.
The latest pre-built binary release is [v0.8.4](https://github.com/sonnyfalk/dataflex-lsp/releases/latest), download `vscode-dataflex-win32-x64-0.8.4.vsix` for Windows, or `vscode-dataflex-darwin-arm64-0.8.4.vsix` for macOS.
In the VSCode Extensions tab, select `Install from VSIX`, and then install the downloaded .vsix file. Now you can open any DataFlex workspace folder, and get full syntax highlighting, code completion, goto definition, etc.

## Building
The LSP server can be built from source together with the VSCode extension, or it can be built standalone. The recommended way is to build it together with editor extensions via the `dataflex-lsp-workspace`, which also builds the VSCode extension.

## Build with dataflex-lsp-workspace
See instructions in [dataflex-lsp-workspace](https://github.com/sonnyfalk/dataflex-lsp-workspace).
