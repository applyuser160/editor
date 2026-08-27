import assert from "node:assert/strict";
import { mkdtemp, mkdir, cp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const repositoryRoot = path.resolve(here, "..", "..");
const hostPath = path.join(repositoryRoot, "extension-host", "host.mjs");
const fixturePath = path.join(
  repositoryRoot,
  "examples",
  "oxide-smoke-command",
);

function waitForEvent(events, predicate, timeoutMs = 5_000) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      cleanup();
      reject(
        new Error(
          `Timed out waiting for extension host event. Received: ${JSON.stringify(events)}`,
        ),
      );
    }, timeoutMs);
    const interval = setInterval(() => {
      const match = events.find(predicate);
      if (match) {
        cleanup();
        resolve(match);
      }
    }, 10);
    function cleanup() {
      clearTimeout(timer);
      clearInterval(interval);
    }
  });
}

const sandbox = await mkdtemp(path.join(os.tmpdir(), "oxide-extension-host-"));
const extensionsRoot = path.join(sandbox, "extensions");
const workspace = path.join(sandbox, "workspace");
const extensionId = "oxide.oxide-smoke-command";
const installedExtensionPath = path.join(
  extensionsRoot,
  extensionId,
  "extension",
);
const events = [];
const stderr = [];

await mkdir(extensionsRoot, { recursive: true });
await mkdir(workspace, { recursive: true });
await cp(fixturePath, installedExtensionPath, { recursive: true });
await writeFile(
  path.join(workspace, "fixture.txt"),
  "trusted workspace fixture\n",
);
await writeFile(
  path.join(extensionsRoot, "installed.json"),
  JSON.stringify([
    {
      id: extensionId,
      name: "Oxide Smoke Command",
      version: "0.1.0",
      description: "Test fixture",
      main: "./extension.js",
      activation_events: [
        "onCommand:oxide.smoke-command",
        "onLanguage:typescript",
      ],
      contributes_commands: [
        {
          command: "oxide.smoke-command",
          title: "Run Oxide Smoke Command",
          category: "Oxide Test",
        },
      ],
      permissions: { workspace_read: true },
      enabled: true,
    },
  ]),
);

const child = spawn(
  process.execPath,
  [
    "--experimental-permission",
    `--allow-fs-read=${extensionsRoot}`,
    `--allow-fs-read=${workspace}`,
    `--allow-fs-read=${hostPath}`,
    hostPath,
    "--extensions-root",
    extensionsRoot,
    "--workspace",
    workspace,
  ],
  { stdio: ["pipe", "pipe", "pipe"] },
);

function send(message) {
  child.stdin.write(`${JSON.stringify(message)}\n`);
}

let buffered = "";
child.stdout.setEncoding("utf8");
child.stdout.on("data", (chunk) => {
  buffered += chunk;
  const lines = buffered.split("\n");
  buffered = lines.pop() || "";
  for (const line of lines) {
    if (!line.trim()) continue;
    const message = JSON.parse(line);
    events.push(message);
    if (message.type !== "request") continue;

    if (message.method === "workspace.fs.readFile") {
      assert.equal(message.params.extensionId, extensionId);
      assert.match(message.params.uri, /fixture\.txt$/);
      send({
        type: "response",
        id: message.id,
        result: {
          base64: Buffer.from("trusted workspace fixture\n").toString("base64"),
        },
      });
    } else if (message.method.startsWith("window.show")) {
      assert.equal(message.params.extensionId, extensionId);
      send({ type: "response", id: message.id, result: null });
    } else {
      send({
        type: "response",
        id: message.id,
        error: "Unexpected RPC method",
      });
    }
  }
});
child.stderr.setEncoding("utf8");
child.stderr.on("data", (chunk) => stderr.push(chunk));

try {
  await waitForEvent(events, (event) => event.type === "host.ready");

  send({
    type: "execute-command",
    command: "oxide.smoke-command",
    args: [],
  });
  await waitForEvent(
    events,
    (event) =>
      event.type === "command.completed" &&
      event.command === "oxide.smoke-command" &&
      event.extensionId === extensionId,
  );
  assert(
    events.some(
      (event) =>
        event.type === "request" &&
        event.method === "workspace.fs.readFile" &&
        event.params.extensionId === extensionId,
    ),
    "The smoke extension must access workspace files through the RPC broker.",
  );
  assert(
    events.some(
      (event) =>
        event.type === "request" &&
        event.method === "window.showInformationMessage" &&
        event.params.message === "Smoke read: trusted workspace fixture\n",
    ),
    "The smoke extension must be able to send an information message through the broker.",
  );

  send({ type: "activate-event", event: "onLanguage:typescript" });
  const completionProvider = await waitForEvent(
    events,
    (event) =>
      event.type === "language.provider.registered" &&
      event.kind === "completion" &&
      event.extensionId === extensionId,
  );
  const hoverProvider = await waitForEvent(
    events,
    (event) =>
      event.type === "language.provider.registered" &&
      event.kind === "hover" &&
      event.extensionId === extensionId,
  );

  send({
    type: "language-provider-request",
    requestId: "completion-request",
    providerId: completionProvider.providerId,
    kind: "completion",
    document: {
      uri: "file:///trusted/example.ts",
      languageId: "typescript",
      version: 1,
      text: "oxi",
    },
    position: { line: 0, character: 3 },
  });
  const completionResult = await waitForEvent(
    events,
    (event) =>
      event.type === "language.provider.result" &&
      event.requestId === "completion-request",
  );
  assert.equal(completionResult.result[0].label, "oxideSmokeCompletion");

  send({
    type: "language-provider-request",
    requestId: "hover-request",
    providerId: hoverProvider.providerId,
    kind: "hover",
    document: {
      uri: "file:///trusted/example.ts",
      languageId: "typescript",
      version: 1,
      text: "oxi",
    },
    position: { line: 0, character: 1 },
  });
  const hoverResult = await waitForEvent(
    events,
    (event) =>
      event.type === "language.provider.result" &&
      event.requestId === "hover-request",
  );
  assert.equal(
    hoverResult.result.contents[0].value,
    "Oxide Extension API v0.1 hover",
  );

  assert.equal(stderr.join(""), "");
  process.stdout.write("Extension host integration test passed.\n");
} finally {
  if (!child.killed && child.exitCode === null) {
    send({ type: "shutdown" });
    await new Promise((resolve) => child.once("exit", resolve));
  }
  await rm(sandbox, { recursive: true, force: true });
}
