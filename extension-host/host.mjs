#!/usr/bin/env node
import { createRequire } from "node:module";
import path from "node:path";
import readline from "node:readline";
import { promises as fs } from "node:fs";

const args = process.argv.slice(2);
const option = (name) => {
  const index = args.indexOf(name);
  return index >= 0 ? args[index + 1] : undefined;
};

const extensionsRoot = option("--extensions-root");
const workspaceRoot = option("--workspace");
if (!extensionsRoot || !workspaceRoot) {
  process.stderr.write(
    "Oxide extension host requires --extensions-root and --workspace.\n",
  );
  process.exit(2);
}

const normalizedExtensionsRoot = path.resolve(extensionsRoot);
const normalizedWorkspaceRoot = path.resolve(workspaceRoot);
const terminateHost = process.exit.bind(process);
const getBuiltinModule = process.getBuiltinModule?.bind(process);
const protocolWrite = process.stdout.write.bind(process.stdout);
const pendingRequests = new Map();
const commandHandlers = new Map();
const activatedExtensions = new Map();
let nextRequestId = 1;
let activeExtensionId = null;

function write(message) {
  protocolWrite(`${JSON.stringify(message)}\n`);
}

// Extensions must use the Oxide RPC-backed workspace API. Node's experimental
// permission model additionally prevents writes and child processes; these guards
// avoid bypassing the broker through direct built-in imports or global fetch.
globalThis.fetch = async () => {
  throw new Error("Network access is not available to Oxide extensions");
};
globalThis.WebSocket = class DisabledWebSocket {
  constructor() {
    throw new Error("Network access is not available to Oxide extensions");
  }
};
process.exit = () => {
  throw new Error("Extensions cannot terminate the Oxide extension host");
};

function event(type, payload = {}) {
  write({ type, ...payload });
}

function ensureInside(root, candidate) {
  const resolved = path.resolve(candidate);
  if (resolved !== root && !resolved.startsWith(`${root}${path.sep}`)) {
    throw new Error("Path escapes its permitted root");
  }
  return resolved;
}

function createDisposable(dispose) {
  return { dispose: typeof dispose === "function" ? dispose : () => {} };
}

class Uri {
  constructor(value) {
    this.value = value;
  }

  toString() {
    return this.value;
  }

  get scheme() {
    return this.value.split(":", 1)[0];
  }

  static file(filePath) {
    const normalized = path.resolve(filePath).replace(/\\/g, "/");
    return new Uri(
      `file://${normalized.startsWith("/") ? "" : "/"}${encodeURI(normalized)}`,
    );
  }

  static parse(value) {
    return new Uri(value);
  }
}

function rpc(method, params) {
  const id = `host-${nextRequestId++}`;
  write({
    type: "request",
    id,
    method,
    params: { ...params, extensionId: activeExtensionId },
  });
  return new Promise((resolve, reject) =>
    pendingRequests.set(id, { resolve, reject }),
  );
}

function requireExtensionId() {
  if (!activeExtensionId) {
    throw new Error(
      "VS Code API was called outside an extension activation or command",
    );
  }
  return activeExtensionId;
}

function createVscodeApi(extensionId) {
  const invokeWithOwner =
    (callback) =>
    async (...args) => {
      const previous = activeExtensionId;
      activeExtensionId = extensionId;
      try {
        return await callback(...args);
      } finally {
        activeExtensionId = previous;
      }
    };

  return {
    Uri,
    Disposable: {
      from: (...disposables) =>
        createDisposable(() =>
          disposables.forEach((item) => item?.dispose?.()),
        ),
    },
    commands: {
      registerCommand(command, callback) {
        if (typeof command !== "string" || typeof callback !== "function") {
          throw new Error(
            "commands.registerCommand requires a command id and callback",
          );
        }
        const ownedCallback = invokeWithOwner(callback);
        commandHandlers.set(command, { extensionId, callback: ownedCallback });
        event("command.registered", { extensionId, command });
        return createDisposable(() => {
          const current = commandHandlers.get(command);
          if (current?.extensionId === extensionId)
            commandHandlers.delete(command);
        });
      },
      executeCommand(command, ...args) {
        return executeCommand(command, args);
      },
    },
    window: {
      showInformationMessage(message) {
        return rpc("window.showInformationMessage", {
          message: String(message),
        });
      },
      showWarningMessage(message) {
        return rpc("window.showWarningMessage", { message: String(message) });
      },
      showErrorMessage(message) {
        return rpc("window.showErrorMessage", { message: String(message) });
      },
    },
    workspace: {
      workspaceFolders: [
        {
          uri: Uri.file(normalizedWorkspaceRoot),
          name: path.basename(normalizedWorkspaceRoot),
          index: 0,
        },
      ],
      fs: {
        async readFile(uri) {
          const value = typeof uri === "string" ? uri : uri?.toString?.();
          const response = await rpc("workspace.fs.readFile", { uri: value });
          if (!response?.base64) return new Uint8Array();
          return Uint8Array.from(Buffer.from(response.base64, "base64"));
        },
      },
      getConfiguration() {
        return {
          get: (_section, defaultValue) => defaultValue,
          has: () => false,
          inspect: () => undefined,
          update: () =>
            Promise.reject(
              new Error(
                "Configuration writes are not supported by Oxide Extension API v0.1",
              ),
            ),
        };
      },
    },
    languages: {
      registerCompletionItemProvider(language, provider) {
        return registerLanguageProvider(
          extensionId,
          "completion",
          language,
          provider,
        );
      },
      registerHoverProvider(language, provider) {
        return registerLanguageProvider(
          extensionId,
          "hover",
          language,
          provider,
        );
      },
    },
    env: { appName: "Oxide Editor", uriScheme: "oxide" },
    version: "0.1.0-oxide-extension-api",
  };
}

const languageProviders = new Map();
function registerLanguageProvider(extensionId, kind, selector, provider) {
  if (!provider || typeof provider !== "object") {
    throw new Error(
      `languages.register${kind}Provider requires a provider object`,
    );
  }
  const key = `${extensionId}:${kind}:${languageProviders.size + 1}`;
  languageProviders.set(key, { extensionId, kind, selector, provider });
  event("language.provider.registered", {
    extensionId,
    kind,
    selector,
    providerId: key,
  });
  return createDisposable(() => languageProviders.delete(key));
}

function blockHighRiskBuiltins(request) {
  const blocked = new Set([
    "child_process",
    "node:child_process",
    "cluster",
    "node:cluster",
    "net",
    "node:net",
    "http",
    "node:http",
    "https",
    "node:https",
    "tls",
    "node:tls",
    "dgram",
    "node:dgram",
    "fs",
    "node:fs",
    "fs/promises",
    "node:fs/promises",
  ]);
  if (blocked.has(request)) {
    throw new Error(
      `The Node built-in '${request}' is not available to Oxide extensions`,
    );
  }
}

const Module = createRequire(import.meta.url)("node:module");
const originalLoad = Module._load;
Module._load = function oxideExtensionModuleLoad(request, parent, isMain) {
  if (request === "vscode") {
    return createVscodeApi(requireExtensionId());
  }
  blockHighRiskBuiltins(request);
  return originalLoad.call(this, request, parent, isMain);
};

if (getBuiltinModule) {
  process.getBuiltinModule = function oxideExtensionBuiltinModule(request) {
    blockHighRiskBuiltins(request);
    return getBuiltinModule(request);
  };
}

async function loadInstalledExtensions() {
  const storePath = path.join(normalizedExtensionsRoot, "installed.json");
  try {
    const raw = await fs.readFile(storePath, "utf8");
    const entries = JSON.parse(raw);
    return Array.isArray(entries)
      ? entries.filter((entry) => entry?.enabled && entry?.main)
      : [];
  } catch (error) {
    if (error.code === "ENOENT") return [];
    throw error;
  }
}

function extensionDirectory(extension) {
  return path.join(normalizedExtensionsRoot, extension.id, "extension");
}

function entryPath(extension) {
  if (
    typeof extension.main !== "string" ||
    extension.main.trim().length === 0
  ) {
    throw new Error(`Extension '${extension.id}' has no Node main entry`);
  }
  const directory = extensionDirectory(extension);
  return ensureInside(directory, path.join(directory, extension.main));
}

function eventActivatesExtension(extension, activationEvent) {
  return (
    extension.activation_events?.includes(activationEvent) ||
    extension.activation_events?.includes("*")
  );
}

async function activateExtension(extension, reason) {
  if (activatedExtensions.has(extension.id))
    return activatedExtensions.get(extension.id);

  const previous = activeExtensionId;
  activeExtensionId = extension.id;
  try {
    const entry = entryPath(extension);
    const extensionRequire = createRequire(entry);
    delete extensionRequire.cache?.[entry];
    const moduleExports = extensionRequire(entry);
    const context = {
      subscriptions: [],
      extensionPath: extensionDirectory(extension),
      extensionUri: Uri.file(extensionDirectory(extension)),
      globalState: {
        get: () => undefined,
        update: () =>
          Promise.reject(
            new Error(
              "globalState is not supported by Oxide Extension API v0.1",
            ),
          ),
      },
      workspaceState: {
        get: () => undefined,
        update: () =>
          Promise.reject(
            new Error(
              "workspaceState is not supported by Oxide Extension API v0.1",
            ),
          ),
      },
    };
    const activationResult =
      typeof moduleExports.activate === "function"
        ? await moduleExports.activate(context)
        : undefined;
    const active = { extension, moduleExports, context, activationResult };
    activatedExtensions.set(extension.id, active);
    event("extension.activated", { extensionId: extension.id, reason });
    return active;
  } catch (error) {
    event("extension.activation-failed", {
      extensionId: extension.id,
      reason,
      message: error instanceof Error ? error.message : String(error),
    });
    throw error;
  } finally {
    activeExtensionId = previous;
  }
}

async function activateForEvent(eventName) {
  const extensions = await loadInstalledExtensions();
  for (const extension of extensions) {
    if (eventActivatesExtension(extension, eventName)) {
      try {
        await activateExtension(extension, eventName);
      } catch {
        // The structured failure event already identifies the extension and error.
      }
    }
  }
}

async function executeCommand(command, args = []) {
  await activateForEvent(`onCommand:${command}`);
  const registration = commandHandlers.get(command);
  if (!registration) {
    event("command.unavailable", {
      command,
      message: "The extension did not register this command after activation.",
    });
    return undefined;
  }
  try {
    const value = await registration.callback(...args);
    event("command.completed", {
      command,
      extensionId: registration.extensionId,
    });
    return value;
  } catch (error) {
    event("command.failed", {
      command,
      extensionId: registration.extensionId,
      message: error instanceof Error ? error.message : String(error),
    });
    throw error;
  }
}

async function reload() {
  for (const [extensionId, active] of activatedExtensions) {
    try {
      const previous = activeExtensionId;
      activeExtensionId = extensionId;
      if (typeof active.moduleExports.deactivate === "function")
        await active.moduleExports.deactivate();
      active.context.subscriptions
        .reverse()
        .forEach((item) => item?.dispose?.());
      activeExtensionId = previous;
    } catch (error) {
      event("extension.deactivation-failed", {
        extensionId,
        message: error instanceof Error ? error.message : String(error),
      });
    }
  }
  commandHandlers.clear();
  languageProviders.clear();
  activatedExtensions.clear();
  event("host.reloaded");
}

function createTextDocument(document) {
  const text = String(document?.text || "");
  const uri = Uri.parse(String(document?.uri || ""));
  const lines = text.split("\n");
  return {
    uri,
    fileName:
      uri.scheme === "file"
        ? decodeURI(uri.toString().replace(/^file:\/\//, ""))
        : uri.toString(),
    languageId: String(document?.languageId || "plaintext"),
    version: Number(document?.version || 1),
    lineCount: lines.length,
    getText: () => text,
    lineAt: (line) => ({ lineNumber: line, text: lines[line] || "" }),
  };
}

async function invokeLanguageProvider(message) {
  const provider = languageProviders.get(message.providerId);
  if (!provider || provider.kind !== message.kind) {
    return { error: "Language provider is unavailable" };
  }
  const method =
    message.kind === "completion" ? "provideCompletionItems" : "provideHover";
  if (typeof provider.provider[method] !== "function") return { result: null };

  const previous = activeExtensionId;
  activeExtensionId = provider.extensionId;
  try {
    const document = createTextDocument(message.document);
    const position = {
      line: Number(message.position?.line || 0),
      character: Number(message.position?.character || 0),
    };
    const result = await provider.provider[method](document, position);
    return { result: serializableLanguageResult(result) };
  } catch (error) {
    return { error: error instanceof Error ? error.message : String(error) };
  } finally {
    activeExtensionId = previous;
  }
}

function serializableLanguageResult(value) {
  if (value === undefined || value === null) return null;
  if (Array.isArray(value)) return value;
  if (Array.isArray(value.items)) return value.items;
  return value;
}

const input = readline.createInterface({
  input: process.stdin,
  crlfDelay: Infinity,
});
input.on("line", (line) => {
  void (async () => {
    let message;
    try {
      message = JSON.parse(line);
    } catch {
      event("host.protocol-error", {
        message: "Received malformed JSON from Oxide core",
      });
      return;
    }

    if (message.type === "response") {
      const pending = pendingRequests.get(message.id);
      if (!pending) return;
      pendingRequests.delete(message.id);
      if (message.error) pending.reject(new Error(message.error));
      else pending.resolve(message.result);
      return;
    }

    switch (message.type) {
      case "reload":
        await reload();
        break;
      case "activate-event":
        await activateForEvent(message.event);
        break;
      case "execute-command":
        await executeCommand(
          message.command,
          Array.isArray(message.args) ? message.args : [],
        );
        break;
      case "language-provider-request": {
        const response = await invokeLanguageProvider(message);
        event("language.provider.result", {
          requestId: message.requestId,
          ...response,
        });
        break;
      }
      case "shutdown":
        await reload();
        input.close();
        terminateHost(0);
        break;
      default:
        event("host.protocol-error", {
          message: `Unknown message type '${message.type}'`,
        });
    }
  })().catch((error) =>
    event("host.unhandled-error", {
      message: error instanceof Error ? error.message : String(error),
    }),
  );
});

process.on("uncaughtException", (error) =>
  event("host.uncaught-exception", { message: error.stack || error.message }),
);
process.on("unhandledRejection", (error) =>
  event("host.unhandled-rejection", { message: String(error) }),
);

event("host.ready", { apiVersion: "0.1", workspace: normalizedWorkspaceRoot });
