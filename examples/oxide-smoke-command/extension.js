const vscode = require("vscode");

function activate(context) {
  let highRiskBuiltinsBlocked = false;
  try {
    require("node:child_process");
  } catch {
    highRiskBuiltinsBlocked = true;
  }
  if (!highRiskBuiltinsBlocked) {
    throw new Error("Oxide must block child_process in Extension API v0.1");
  }

  let directFilesystemBlocked = false;
  try {
    process.getBuiltinModule?.("node:fs");
  } catch {
    directFilesystemBlocked = true;
  }
  if (!directFilesystemBlocked) {
    throw new Error(
      "Oxide must block direct filesystem access in Extension API v0.1",
    );
  }

  context.subscriptions.push(
    vscode.commands.registerCommand("oxide.smoke-command", async () => {
      const bytes = await vscode.workspace.fs.readFile(
        vscode.Uri.file(
          `${vscode.workspace.workspaceFolders[0].uri.fileName}/fixture.txt`,
        ),
      );
      const text = Buffer.from(bytes).toString("utf8");
      await vscode.window.showInformationMessage(`Smoke read: ${text}`);
      return text;
    }),
    vscode.languages.registerCompletionItemProvider("typescript", {
      provideCompletionItems() {
        return [
          {
            label: "oxideSmokeCompletion",
            insertText: "oxideSmokeCompletion",
            kind: 3,
          },
        ];
      },
    }),
    vscode.languages.registerHoverProvider("typescript", {
      provideHover() {
        return { contents: [{ value: "Oxide Extension API v0.1 hover" }] };
      },
    }),
  );
}

function deactivate() {}

module.exports = { activate, deactivate };
