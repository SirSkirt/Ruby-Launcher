const STORAGE_KEYS = {
  settings: "ruby-launcher.settings.v2",
  instances: "ruby-launcher.instances.v1",
  logs: "ruby-launcher.logs.v1"
};

const DEFAULT_SETTINGS = {
  memoryMb: 2048,
  javaRuntime: "automatic",
  renderer: "automatic",
  keepScreenOn: true,
  showLogs: true,
  allowCellular: false,
  selectedInstanceId: null,
  controllerDeadzone: 0.22,
  controllerVibration: true,
  controllerSupportEnabled: true,
  launcherFullscreen: true,
  controllerPromptOnStart: true,
  hideLauncherWhilePlaying: true
};

const VIEW_META = {
  home: { eyebrow: "LAUNCHER HOME", heading: "Good to see you." },
  instances: { eyebrow: "GAME PROFILES", heading: "Choose your world." },
  versions: { eyebrow: "MINECRAFT CATALOG", heading: "Build an environment." },
  modpacks: { eyebrow: "MODRINTH MODPACKS", heading: "Find a ready-made setup." },
  settings: { eyebrow: "LAUNCHER SETTINGS", heading: "Tune Ruby Launcher." }
};

const VIEW_ORDER = ["home", "instances", "versions", "modpacks", "settings"];
const FOCUSABLE_SELECTOR = [
  "button:not([disabled])",
  "input:not([disabled])",
  "select:not([disabled])",
  "[data-gamepad-focus]:not([aria-disabled='true'])"
].join(",");

const state = {
  launcherInfo: null,
  settings: { ...DEFAULT_SETTINGS },
  instances: [],
  selectedInstance: null,
  currentView: "home",
  account: null,
  versionCatalog: null,
  authPollTimer: null,
  authVerificationUri: "https://microsoft.com/devicelogin",
  installRunning: false,
  modrinth: { results: [], selectedProject: null, versions: [] },
  controller: {
    index: null,
    id: "",
    connected: false,
    previousButtons: [],
    previousAxes: [0, 0],
    lastMoveAt: 0,
    lastMoveDirection: null,
    repeatStarted: false,
    startMenuPendingAt: null,
    startChordTriggered: false,
    startupPromptResolved: false
  }
};

const elements = {
  controllerGate: document.querySelector("#controllerGate"),
  controllerGateStatus: document.querySelector("#controllerGateStatus"),
  appShell: document.querySelector("#appShell"),
  controllerName: document.querySelector("#controllerName"),
  settingsControllerName: document.querySelector("#settingsControllerName"),
  viewEyebrow: document.querySelector("#viewEyebrow"),
  viewHeading: document.querySelector("#viewHeading"),
  views: [...document.querySelectorAll("[data-view]")],
  navButtons: [...document.querySelectorAll("[data-view-target]")],
  selectedName: document.querySelector("#selectedName"),
  selectedDetails: document.querySelector("#selectedDetails"),
  selectedStatus: document.querySelector("#selectedStatus"),
  instanceList: document.querySelector("#instanceList"),
  instanceEmptyState: document.querySelector("#instanceEmptyState"),
  instanceDialog: document.querySelector("#instanceDialog"),
  instanceForm: document.querySelector("#instanceForm"),
  instanceName: document.querySelector("#instanceName"),
  minecraftVersion: document.querySelector("#minecraftVersion"),
  loaderType: document.querySelector("#loaderType"),
  profileDialog: document.querySelector("#profileDialog"),
  profileForm: document.querySelector("#profileForm"),
  profileDialogTitle: document.querySelector("#profileDialogTitle"),
  profileInstanceId: document.querySelector("#profileInstanceId"),
  profileName: document.querySelector("#profileName"),
  profileMemory: document.querySelector("#profileMemory"),
  profileMemoryValue: document.querySelector("#profileMemoryValue"),
  profileInitialMemory: document.querySelector("#profileInitialMemory"),
  profileInitialMemoryValue: document.querySelector("#profileInitialMemoryValue"),
  profileJavaRuntime: document.querySelector("#profileJavaRuntime"),
  profileRenderer: document.querySelector("#profileRenderer"),
  profileJvmPreset: document.querySelector("#profileJvmPreset"),
  profileControllerSupport: document.querySelector("#profileControllerSupport"),
  profileGameFullscreen: document.querySelector("#profileGameFullscreen"),
  profileResolutionWidth: document.querySelector("#profileResolutionWidth"),
  profileResolutionHeight: document.querySelector("#profileResolutionHeight"),
  profileJvmPreview: document.querySelector("#profileJvmPreview"),
  launcherMenuDialog: document.querySelector("#launcherMenuDialog"),
  gameLibraryDialog: document.querySelector("#gameLibraryDialog"),
  logsDialog: document.querySelector("#logsDialog"),
  logOutput: document.querySelector("#logOutput"),
  settingsForm: document.querySelector("#settingsForm"),
  memoryRange: document.querySelector("#memoryRange"),
  memoryValue: document.querySelector("#memoryValue"),
  javaRuntime: document.querySelector("#javaRuntime"),
  renderer: document.querySelector("#renderer"),
  keepScreenOn: document.querySelector("#keepScreenOn"),
  showLogs: document.querySelector("#showLogs"),
  allowCellular: document.querySelector("#allowCellular"),
  controllerDeadzone: document.querySelector("#controllerDeadzone"),
  deadzoneValue: document.querySelector("#deadzoneValue"),
  controllerVibration: document.querySelector("#controllerVibration"),
  controllerSupportEnabled: document.querySelector("#controllerSupportEnabled"),
  launcherFullscreen: document.querySelector("#launcherFullscreen"),
  controllerPromptOnStart: document.querySelector("#controllerPromptOnStart"),
  hideLauncherWhilePlaying: document.querySelector("#hideLauncherWhilePlaying"),
  testVibrationButton: document.querySelector("#testVibrationButton"),
  accountPillName: document.querySelector("#accountPillName"),
  accountPillStatus: document.querySelector("#accountPillStatus"),
  accountTitle: document.querySelector("#accountTitle"),
  accountDescription: document.querySelector("#accountDescription"),
  accountSignInButton: document.querySelector("#accountSignInButton"),
  authDialog: document.querySelector("#authDialog"),
  authUserCode: document.querySelector("#authUserCode"),
  authVerificationUri: document.querySelector("#authVerificationUri"),
  authInstructions: document.querySelector("#authInstructions"),
  authStatus: document.querySelector("#authStatus"),
  openAuthPageButton: document.querySelector("#openAuthPageButton"),
  installDialog: document.querySelector("#installDialog"),
  installTitle: document.querySelector("#installTitle"),
  installStage: document.querySelector("#installStage"),
  installPercent: document.querySelector("#installPercent"),
  installProgressBar: document.querySelector("#installProgressBar"),
  installBytes: document.querySelector("#installBytes"),
  installFiles: document.querySelector("#installFiles"),
  installDetail: document.querySelector("#installDetail"),
  closeInstallButton: document.querySelector("#closeInstallButton"),
  rememberDialog: document.querySelector("#rememberDialog"),
  rememberYesButton: document.querySelector("#rememberYesButton"),
  rememberNoButton: document.querySelector("#rememberNoButton"),
  latestReleaseValue: document.querySelector("#latestReleaseValue"),
  latestSnapshotValue: document.querySelector("#latestSnapshotValue"),
  catalogStatus: document.querySelector("#catalogStatus"),
  catalogList: document.querySelector("#catalogList"),
  versionOptions: document.querySelector("#versionOptions"),
  modrinthSearchForm: document.querySelector("#modrinthSearchForm"),
  modrinthQuery: document.querySelector("#modrinthQuery"),
  modrinthSort: document.querySelector("#modrinthSort"),
  modrinthStatus: document.querySelector("#modrinthStatus"),
  modrinthResults: document.querySelector("#modrinthResults"),
  modrinthEmptyState: document.querySelector("#modrinthEmptyState"),
  modrinthDialog: document.querySelector("#modrinthDialog"),
  modrinthDialogTitle: document.querySelector("#modrinthDialogTitle"),
  modrinthDialogIcon: document.querySelector("#modrinthDialogIcon"),
  modrinthDialogDescription: document.querySelector("#modrinthDialogDescription"),
  modrinthDialogAuthor: document.querySelector("#modrinthDialogAuthor"),
  modrinthVersionSelect: document.querySelector("#modrinthVersionSelect"),
  modrinthControllerSupport: document.querySelector("#modrinthControllerSupport"),
  modrinthVersionDetails: document.querySelector("#modrinthVersionDetails"),
  installModrinthPack: document.querySelector("#installModrinthPack"),
  toastRegion: document.querySelector("#toastRegion")
};

function isTauriRuntime() {
  return Boolean(window.__TAURI__?.core?.invoke);
}

async function invoke(command, args = {}) {
  if (isTauriRuntime()) return window.__TAURI__.core.invoke(command, args);
  return browserFallback(command, args);
}

function readJson(key, fallback) {
  try {
    const raw = localStorage.getItem(key);
    return raw ? JSON.parse(raw) : structuredClone(fallback);
  } catch {
    return structuredClone(fallback);
  }
}

function writeJson(key, value) {
  localStorage.setItem(key, JSON.stringify(value));
}

function fallbackLog(message) {
  try {
    const logs = readJson(STORAGE_KEYS.logs, []);
    logs.push(`[${new Date().toISOString()}] ${message}`);
    writeJson(STORAGE_KEYS.logs, logs.slice(-200));
  } catch {
    // Logging must never interrupt controller detection or launcher navigation.
  }
}

function slugify(value) {
  return value.toLowerCase().trim().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 40) || "instance";
}

async function browserFallback(command, args) {
  switch (command) {
    case "launcher_info":
      return { name: "Ruby Launcher", version: "0.21.0", build: "b2026.21", platform: "Browser preview", nativeBackend: false };
    case "load_settings":
      return readJson(STORAGE_KEYS.settings, DEFAULT_SETTINGS);
    case "save_settings":
      writeJson(STORAGE_KEYS.settings, args.settings);
      fallbackLog("Settings saved in browser preview storage.");
      return args.settings;
    case "list_instances":
      return readJson(STORAGE_KEYS.instances, []);
    case "create_instance": {
      const instances = readJson(STORAGE_KEYS.instances, []);
      const baseId = slugify(args.request.name);
      let id = baseId;
      let suffix = 2;
      while (instances.some((instance) => instance.id === id)) id = `${baseId}-${suffix++}`;
      const instance = {
        id,
        name: args.request.name.trim(),
        minecraftVersion: args.request.minecraftVersion.trim(),
        loader: args.request.loader,
        status: "Not installed",
        gameDir: `browser-preview/instances/${id}/.minecraft`,
        createdAt: new Date().toISOString(),
        memoryMb: state.settings.memoryMb,
        initialMemoryMb: Math.min(512, state.settings.memoryMb),
        javaRuntime: state.settings.javaRuntime,
        renderer: state.settings.renderer,
        jvmPreset: "balanced",
        controllerSupportEnabled: state.settings.controllerSupportEnabled,
        gameFullscreen: true,
        resolutionWidth: 1280,
        resolutionHeight: 720,
        source: "manual",
        loaderVersion: null,
        launchVersion: null,
        modpackProjectId: null,
        modpackVersionId: null,
        modpackName: null
      };
      instances.push(instance);
      writeJson(STORAGE_KEYS.instances, instances);
      fallbackLog(`Created browser-preview instance '${instance.name}'.`);
      return instance;
    }
    case "delete_instance": {
      const instances = readJson(STORAGE_KEYS.instances, []);
      writeJson(STORAGE_KEYS.instances, instances.filter((instance) => instance.id !== args.instanceId));
      fallbackLog(`Removed browser-preview metadata for '${args.instanceId}'.`);
      return true;
    }
    case "update_instance": {
      const instances = readJson(STORAGE_KEYS.instances, []);
      const instance = instances.find((entry) => entry.id === args.request.instanceId);
      if (!instance) throw new Error("Instance not found.");
      Object.assign(instance, {
        name: args.request.name.trim(),
        memoryMb: args.request.memoryMb,
        initialMemoryMb: args.request.initialMemoryMb,
        javaRuntime: args.request.javaRuntime,
        renderer: args.request.renderer,
        jvmPreset: args.request.jvmPreset,
        controllerSupportEnabled: args.request.controllerSupportEnabled,
        gameFullscreen: args.request.gameFullscreen,
        resolutionWidth: args.request.resolutionWidth,
        resolutionHeight: args.request.resolutionHeight
      });
      writeJson(STORAGE_KEYS.instances, instances);
      return instance;
    }
    case "prepare_instance": {
      const instance = readJson(STORAGE_KEYS.instances, []).find((entry) => entry.id === args.instanceId);
      if (!instance) throw new Error("Instance not found.");
      return { instanceId: instance.id, message: "Instance directories require the native Tauri application." };
    }
    case "search_modrinth_modpacks":
      return {
        hits: [
          { projectId: "AANobbMI", slug: "sodium", title: "Preview Performance Pack", description: "A browser-preview Modrinth result.", author: "Modrinth", downloads: 1250000, iconUrl: "", latestVersion: "1.21.1", versions: ["1.21.1"], categories: ["fabric", "optimization"] }
        ],
        totalHits: 1,
        offset: 0,
        limit: 20
      };
    case "list_modrinth_modpack_versions":
      return [
        { id: "preview-version", projectId: args.projectId, name: "Preview 1.0", versionNumber: "1.0.0", gameVersions: ["1.21.1"], loaders: ["fabric"], versionType: "release", datePublished: new Date().toISOString(), downloads: 5000 }
      ];
    case "install_modrinth_modpack": {
      const instances = readJson(STORAGE_KEYS.instances, []);
      const request = args.request;
      const id = `${slugify(request.name)}-${instances.length + 1}`;
      const instance = {
        id, name: request.name, minecraftVersion: "1.21.1", loader: "fabric", status: "Installed",
        gameDir: `browser-preview/instances/${id}/.minecraft`, createdAt: new Date().toISOString(),
        memoryMb: state.settings.memoryMb, initialMemoryMb: 512, javaRuntime: state.settings.javaRuntime,
        renderer: state.settings.renderer, jvmPreset: "balanced", controllerSupportEnabled: request.controllerSupportEnabled,
        gameFullscreen: true, resolutionWidth: 1280, resolutionHeight: 720, source: "modrinth",
        loaderVersion: "0.16.0", launchVersion: "fabric-loader-preview", modpackProjectId: request.projectId,
        modpackVersionId: request.versionId, modpackName: request.name
      };
      instances.push(instance);
      writeJson(STORAGE_KEYS.instances, instances);
      return instance;
    }
    case "fetch_version_catalog":
      return {
        latestRelease: "26.1.2",
        latestSnapshot: "26.2-snapshot",
        versions: [
          { id: "26.1.2", type: "release", releaseTime: "2026-04-09T00:00:00Z" },
          { id: "1.21.11", type: "release", releaseTime: "2025-12-01T00:00:00Z" },
          { id: "1.21.1", type: "release", releaseTime: "2024-08-08T00:00:00Z" }
        ]
      };
    case "install_vanilla": {
      const instances = readJson(STORAGE_KEYS.instances, []);
      const instance = instances.find((entry) => entry.id === args.instanceId);
      if (!instance) throw new Error("Instance not found.");
      if (instance.loader !== "none") throw new Error("The vanilla installer only supports profiles with no third-party loader.");
      instance.status = "Installed";
      writeJson(STORAGE_KEYS.instances, instances);
      fallbackLog(`Simulated vanilla installation for '${instance.name}' in browser preview.`);
      return { instanceId: instance.id, version: instance.minecraftVersion, javaMajorVersion: 21, downloadedBytes: 0, files: 0, message: `Preview complete: Minecraft ${instance.minecraftVersion} marked installed.` };
    }
    case "get_account":
      return null;
    case "restore_saved_account":
      return readJson("ruby-launcher.preview-account", null);
    case "start_microsoft_sign_in":
      return { userCode: "RUBY-204", verificationUri: "https://microsoft.com/devicelogin", expiresIn: 900, interval: 5, message: "Use the preview code to test the dialog layout." };
    case "poll_microsoft_sign_in": {
      const account = { id: "preview-profile", name: "RubyPlayer", entitlementCount: 1, sessionOnly: true };
      return { status: "signedIn", message: "Signed in as RubyPlayer (browser preview).", retryAfter: 0, account };
    }
    case "set_remember_me": {
      const account = { ...(state.account || { id: "preview-profile", name: "RubyPlayer", entitlementCount: 1 }), sessionOnly: !args.remember };
      if (args.remember) writeJson("ruby-launcher.preview-account", account);
      else localStorage.removeItem("ruby-launcher.preview-account");
      return account;
    }
    case "cancel_microsoft_sign_in":
      return null;
    case "sign_out":
      localStorage.removeItem("ruby-launcher.preview-account");
      return null;
    case "open_external_url":
      window.open(args.url, "_blank", "noopener,noreferrer");
      return null;
    case "play_instance": {
      const instance = state.instances.find((item) => item.id === args.instanceId);
      if (!instance) throw new Error("Instance not found.");
      if (!state.account) throw new Error("Sign in before playing.");
      instance.status = "Running";
      fallbackLog(`Simulated Play flow for '${instance.name}': files, runtime, and game launch.`);
      return { instanceId: instance.id, pid: 202605, javaMajorVersion: 21, message: `Preview: Minecraft ${instance.minecraftVersion} is starting.` };
    }
    case "exit_launcher":
      fallbackLog("Exit requested from the launcher menu. Browser preview remains open.");
      return { exited: false };
    case "read_launcher_log":
      return readJson(STORAGE_KEYS.logs, [
        `[${new Date().toISOString()}] Ruby Launcher browser preview started.`,
        "Native Tauri downloads and authentication are simulated in a normal browser tab."
      ]).join("\n");
    default:
      throw new Error(`Unknown preview command: ${command}`);
  }
}

function loaderLabel(loader) {
  return ({ none: "No third-party loader", fabric: "Fabric", quilt: "Quilt", forge: "Forge", neoforge: "NeoForge" })[loader] ?? loader;
}

function instanceInitial(instance) {
  return ({ none: "R", fabric: "F", quilt: "Q", forge: "G", neoforge: "N" })[instance.loader] ?? "R";
}

function cleanControllerName(name) {
  if (!name) return "Gamepad";
  return name.replace(/\([^)]*vendor:[^)]*\)/gi, "").replace(/\s+/g, " ").trim().slice(0, 46) || "Gamepad";
}

function activeDialog() {
  return [...document.querySelectorAll("dialog[open]")].at(-1) ?? null;
}

function getFocusScope() {
  return activeDialog() ?? document.querySelector(`.view[data-view="${state.currentView}"]`) ?? elements.appShell;
}

function visibleFocusable(scope = getFocusScope()) {
  return [...scope.querySelectorAll(FOCUSABLE_SELECTOR)].filter((element) => {
    const style = window.getComputedStyle(element);
    const rect = element.getBoundingClientRect();
    return style.visibility !== "hidden" && style.display !== "none" && rect.width > 0 && rect.height > 0 && !element.closest("[inert]");
  });
}

function markGamepadFocus(element) {
  document.querySelectorAll(".gamepad-focus").forEach((entry) => entry.classList.remove("gamepad-focus"));
  if (!element) return;
  element.classList.add("gamepad-focus");
  element.focus({ preventScroll: true });
  element.scrollIntoView({ block: "nearest", inline: "nearest", behavior: "smooth" });
}

function focusFirst(scope = getFocusScope()) {
  const candidates = visibleFocusable(scope);
  const preferred = candidates.find((element) => element.hasAttribute("data-default-focus"));
  const first = preferred ?? candidates[0];
  if (first) markGamepadFocus(first);
}

function adjustFocusedControl(direction) {
  if (!["left", "right"].includes(direction)) return false;
  const target = document.activeElement;

  if (target instanceof HTMLInputElement && target.type === "range") {
    if (direction === "left") target.stepDown();
    else target.stepUp();
    target.dispatchEvent(new Event("input", { bubbles: true }));
    pulseController(12, 0.12);
    return true;
  }

  if (target instanceof HTMLSelectElement) {
    const offset = direction === "left" ? -1 : 1;
    const nextIndex = Math.max(0, Math.min(target.options.length - 1, target.selectedIndex + offset));
    if (nextIndex !== target.selectedIndex) {
      target.selectedIndex = nextIndex;
      target.dispatchEvent(new Event("change", { bubbles: true }));
      pulseController(12, 0.12);
    }
    return true;
  }

  return false;
}

function navigateDirection(direction) {
  if (!adjustFocusedControl(direction)) moveFocus(direction);
}

function moveFocus(direction) {
  const candidates = visibleFocusable();
  if (!candidates.length) return;
  const current = candidates.includes(document.activeElement) ? document.activeElement : candidates[0];
  const currentRect = current.getBoundingClientRect();
  const currentCenter = { x: currentRect.left + currentRect.width / 2, y: currentRect.top + currentRect.height / 2 };

  let best = null;
  let bestScore = Number.POSITIVE_INFINITY;
  for (const candidate of candidates) {
    if (candidate === current) continue;
    const rect = candidate.getBoundingClientRect();
    const center = { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
    const dx = center.x - currentCenter.x;
    const dy = center.y - currentCenter.y;
    const valid = direction === "left" ? dx < -8 : direction === "right" ? dx > 8 : direction === "up" ? dy < -8 : dy > 8;
    if (!valid) continue;
    const primary = direction === "left" || direction === "right" ? Math.abs(dx) : Math.abs(dy);
    const secondary = direction === "left" || direction === "right" ? Math.abs(dy) : Math.abs(dx);
    const score = primary + secondary * 2.25;
    if (score < bestScore) {
      best = candidate;
      bestScore = score;
    }
  }

  if (best) {
    markGamepadFocus(best);
    pulseController(12, 0.12);
  }
}

function activateFocused() {
  const target = document.activeElement;
  if (!target || !visibleFocusable().includes(target)) return;
  if (target instanceof HTMLInputElement && ["checkbox", "radio"].includes(target.type)) target.click();
  else if (target instanceof HTMLSelectElement) target.focus();
  else target.click();
  pulseController(22, 0.2);
}

function navigate(viewName, focus = true) {
  if (!VIEW_ORDER.includes(viewName)) return;
  state.currentView = viewName;
  for (const view of elements.views) view.classList.toggle("is-active", view.dataset.view === viewName);
  for (const button of elements.navButtons) button.classList.toggle("is-active", button.dataset.viewTarget === viewName);
  const meta = VIEW_META[viewName];
  elements.viewEyebrow.textContent = meta.eyebrow;
  elements.viewHeading.textContent = meta.heading;
  document.querySelector("#mainContent").scrollTo({ top: 0, behavior: "smooth" });
  if (focus) requestAnimationFrame(() => focusFirst());
}

function cycleView(offset) {
  const index = VIEW_ORDER.indexOf(state.currentView);
  navigate(VIEW_ORDER[(index + offset + VIEW_ORDER.length) % VIEW_ORDER.length]);
}

function handleBack() {
  const dialog = activeDialog();
  if (dialog === elements.installDialog && state.installRunning) return;
  if (dialog === elements.authDialog) {
    cancelMicrosoftSignIn();
    return;
  }
  if (dialog === elements.rememberDialog) {
    chooseRememberMe(false);
    return;
  }
  if (dialog) {
    dialog.close();
    requestAnimationFrame(() => focusFirst());
    return;
  }
  if (state.currentView !== "home") navigate("home");
}

function updateControllerIdentity(gamepad) {
  const name = cleanControllerName(gamepad?.id);
  elements.controllerName.textContent = name;
  elements.settingsControllerName.textContent = name;
}

function unlockLauncherWithoutController() {
  const wasLocked = !elements.controllerGate.classList.contains("is-unlocked") || elements.appShell.hasAttribute("inert");
  state.controller.startupPromptResolved = true;
  elements.controllerGate.classList.add("is-unlocked");
  elements.appShell.removeAttribute("inert");
  elements.appShell.setAttribute("aria-hidden", "false");
  if (!state.controller.connected) {
    elements.controllerName.textContent = "No controller connected";
    elements.settingsControllerName.textContent = "No controller connected";
  }
  if (wasLocked) requestAnimationFrame(() => focusFirst());
}

function applyControllerPromptSetting() {
  state.controller.startupPromptResolved = false;
  if (state.settings.controllerPromptOnStart) {
    const gamepad = findController();
    if (gamepad) unlockForController(gamepad);
    else lockForController(true);
  } else {
    unlockLauncherWithoutController();
  }
}

function unlockForController(gamepad) {
  if (!gamepad) return;
  const wasConnected = state.controller.connected;
  state.controller.index = gamepad.index;
  state.controller.id = gamepad.id;
  state.controller.connected = true;
  state.controller.startupPromptResolved = true;
  state.controller.previousButtons = gamepad.buttons.map((button) => button.pressed);
  state.controller.previousAxes = [gamepad.axes[0] || 0, gamepad.axes[1] || 0];
  updateControllerIdentity(gamepad);
  elements.controllerGateStatus.innerHTML = `<span class="status-pulse" aria-hidden="true"></span><span>${cleanControllerName(gamepad.id)} connected</span>`;
  elements.controllerGate.classList.add("is-unlocked");
  elements.appShell.removeAttribute("inert");
  elements.appShell.setAttribute("aria-hidden", "false");
  if (!wasConnected) {
    fallbackLog(`Controller connected: ${cleanControllerName(gamepad.id)}.`);
    setTimeout(() => focusFirst(), 250);
  }
}

function lockForController(force = false) {
  const wasConnected = state.controller.connected;
  state.controller.index = null;
  state.controller.id = "";
  state.controller.connected = false;
  state.controller.previousButtons = [];
  state.controller.lastMoveDirection = null;
  state.controller.repeatStarted = false;
  document.querySelectorAll(".gamepad-focus").forEach((entry) => entry.classList.remove("gamepad-focus"));

  const shouldBlockForStartup = force
    && state.settings.controllerPromptOnStart
    && !state.controller.startupPromptResolved;
  if (shouldBlockForStartup) {
    elements.controllerGateStatus.innerHTML = '<span class="status-pulse" aria-hidden="true"></span><span>Waiting for a gamepad…</span>';
    elements.controllerGate.classList.remove("is-unlocked");
    elements.appShell.setAttribute("inert", "");
    elements.appShell.setAttribute("aria-hidden", "true");
    return;
  }

  // The controller prompt is a startup gate, not a permanent launcher lock.
  // WebView2 can briefly stop exposing gamepads while the window is minimized;
  // keeping the shell active prevents a tray restore from becoming unclickable.
  state.controller.startupPromptResolved = true;
  elements.controllerGateStatus.innerHTML = '<span class="status-pulse" aria-hidden="true"></span><span>Controller disconnected. Mouse and keyboard remain available.</span>';
  elements.controllerGate.classList.add("is-unlocked");
  elements.appShell.removeAttribute("inert");
  elements.appShell.setAttribute("aria-hidden", "false");
  elements.controllerName.textContent = "No controller connected";
  elements.settingsControllerName.textContent = "No controller connected";
  if (wasConnected) fallbackLog("Controller disconnected; launcher input remains available.");
}

function recoverLauncherInput() {
  const gamepad = findController();
  if (gamepad) {
    unlockForController(gamepad);
  } else {
    lockForController(false);
  }

  // Clear stale modal/focus state left behind while WebView2 was minimized.
  const focused = document.activeElement;
  if (focused instanceof HTMLElement && focused.closest("[inert]")) focused.blur();
  requestAnimationFrame(() => {
    focusFirst();
    requestAnimationFrame(() => focusFirst());
  });

  try {
    const currentWebview = window.__TAURI__?.webview?.getCurrentWebview?.();
    currentWebview?.setFocus?.().catch(() => {});
  } catch {
    // Native Rust focus restoration remains the primary path.
  }
}


function connectedGamepads() {
  if (!navigator.getGamepads) return [];
  return [...navigator.getGamepads()].filter(Boolean);
}

function findController() {
  const pads = connectedGamepads();
  if (!pads.length) return null;
  if (state.controller.index !== null) return pads.find((pad) => pad.index === state.controller.index) ?? pads[0];
  return pads[0];
}

async function pulseController(duration = 18, magnitude = 0.16) {
  if (!state.settings.controllerVibration) return;
  const gamepad = findController();
  const actuator = gamepad?.vibrationActuator;
  if (!actuator?.playEffect) return;
  try {
    await actuator.playEffect("dual-rumble", { duration, strongMagnitude: magnitude, weakMagnitude: magnitude * 0.7 });
  } catch {
    // Vibration is optional and unsupported on many browser/controller combinations.
  }
}

function buttonPressed(gamepad, index) {
  return Boolean(gamepad.buttons[index]?.pressed);
}

function edgePressed(gamepad, index) {
  const pressed = buttonPressed(gamepad, index);
  const previous = Boolean(state.controller.previousButtons[index]);
  state.controller.previousButtons[index] = pressed;
  return pressed && !previous;
}

function axisDirection(gamepad) {
  const deadzone = Number(state.settings.controllerDeadzone) || DEFAULT_SETTINGS.controllerDeadzone;
  const x = gamepad.axes[0] || 0;
  const y = gamepad.axes[1] || 0;
  const dpad = {
    left: buttonPressed(gamepad, 14),
    right: buttonPressed(gamepad, 15),
    up: buttonPressed(gamepad, 12),
    down: buttonPressed(gamepad, 13)
  };
  if (dpad.left || x < -deadzone) return "left";
  if (dpad.right || x > deadzone) return "right";
  if (dpad.up || y < -deadzone) return "up";
  if (dpad.down || y > deadzone) return "down";
  return null;
}

function handleDirectionalRepeat(direction, now) {
  if (!direction) {
    state.controller.lastMoveDirection = null;
    state.controller.repeatStarted = false;
    return;
  }

  const changed = direction !== state.controller.lastMoveDirection;
  const elapsed = now - state.controller.lastMoveAt;
  const threshold = changed ? 0 : state.controller.repeatStarted ? 125 : 420;
  if (changed || elapsed >= threshold) {
    navigateDirection(direction);
    state.controller.lastMoveAt = now;
    state.controller.lastMoveDirection = direction;
    if (!changed) state.controller.repeatStarted = true;
  }
}

function pollController(now) {
  const gamepad = findController();
  if (!gamepad) {
    lockForController();
    requestAnimationFrame(pollController);
    return;
  }

  if (!state.controller.connected || gamepad.index !== state.controller.index) unlockForController(gamepad);
  handleDirectionalRepeat(axisDirection(gamepad), now);

  const startWasDown = Boolean(state.controller.previousButtons[9]);
  const startDown = buttonPressed(gamepad, 9);
  const rightBumperDown = buttonPressed(gamepad, 5);
  const activateEdge = edgePressed(gamepad, 0);
  const backEdge = edgePressed(gamepad, 1);
  const leftBumperEdge = edgePressed(gamepad, 4);
  const rightBumperEdge = edgePressed(gamepad, 5);
  const startEdge = edgePressed(gamepad, 9);
  const startReleased = startWasDown && !startDown;

  let suppressRightBumperNavigation = false;
  if ((startEdge && rightBumperDown) || (rightBumperEdge && startDown)) {
    state.controller.startMenuPendingAt = null;
    state.controller.startChordTriggered = true;
    suppressRightBumperNavigation = true;
    openLogs();
  } else if (startEdge) {
    state.controller.startMenuPendingAt = now;
    state.controller.startChordTriggered = false;
  }

  if (startReleased) {
    if (state.controller.startMenuPendingAt !== null && !state.controller.startChordTriggered) openLauncherMenu();
    state.controller.startMenuPendingAt = null;
    state.controller.startChordTriggered = false;
  } else if (
    startDown &&
    state.controller.startMenuPendingAt !== null &&
    now - state.controller.startMenuPendingAt >= 320 &&
    !state.controller.startChordTriggered
  ) {
    openLauncherMenu();
    state.controller.startMenuPendingAt = null;
  }

  if (activateEdge) activateFocused();
  if (backEdge) handleBack();
  if (leftBumperEdge && !startDown) cycleView(-1);
  if (rightBumperEdge && !startDown && !suppressRightBumperNavigation) cycleView(1);

  requestAnimationFrame(pollController);
}

function selectInstanceLocally(instanceId) {
  const selected = state.instances.find((instance) => instance.id === instanceId) ?? state.instances[0] ?? null;
  state.selectedInstance = selected;
  state.settings.selectedInstanceId = selected?.id ?? null;
  renderSelectedInstance();
  renderInstances();
}

async function persistSelection(instanceId) {
  selectInstanceLocally(instanceId);
  try {
    await invoke("save_settings", { settings: state.settings });
  } catch (error) {
    showToast(`Selection changed, but could not be saved: ${errorMessage(error)}`, true);
  }
}

function renderSelectedInstance() {
  const instance = state.selectedInstance;
  if (!instance) {
    elements.selectedName.textContent = "No instance selected";
    elements.selectedDetails.textContent = "Create an instance to continue";
    elements.selectedStatus.textContent = "Empty";
    return;
  }
  elements.selectedName.textContent = instance.name;
  elements.selectedDetails.textContent = `Minecraft ${instance.minecraftVersion} · ${loaderLabel(instance.loader)}`;
  elements.selectedStatus.textContent = instance.status || "Profile";
}

function renderInstances() {
  elements.instanceList.replaceChildren();
  elements.instanceEmptyState.hidden = state.instances.length > 0;

  for (const instance of state.instances) {
    const row = document.createElement("article");
    row.className = "instance-row";
    if (instance.id === state.selectedInstance?.id) row.classList.add("is-selected");

    const icon = document.createElement("span");
    icon.className = "instance-row-icon";
    icon.textContent = instanceInitial(instance);

    const copy = document.createElement("div");
    copy.className = "instance-row-copy";
    const name = document.createElement("strong");
    name.textContent = instance.name;
    const details = document.createElement("span");
    const sourceLabel = instance.source === "modrinth" ? " · Modrinth" : "";
    details.textContent = `Minecraft ${instance.minecraftVersion} · ${loaderLabel(instance.loader)}${sourceLabel} · ${instance.status || "Profile"}`;
    copy.append(name, details);

    const actions = document.createElement("div");
    actions.className = "instance-row-actions";
    const selectButton = document.createElement("button");
    selectButton.className = "mini-button";
    selectButton.type = "button";
    selectButton.textContent = instance.id === state.selectedInstance?.id ? "Selected" : "Select";
    selectButton.disabled = instance.id === state.selectedInstance?.id;
    selectButton.addEventListener("click", () => persistSelection(instance.id));

    const editButton = document.createElement("button");
    editButton.className = "mini-button";
    editButton.type = "button";
    editButton.textContent = "Edit";
    editButton.addEventListener("click", () => openProfileEditor(instance));

    const deleteButton = document.createElement("button");
    deleteButton.className = "mini-button danger";
    deleteButton.type = "button";
    deleteButton.textContent = "Remove";
    deleteButton.addEventListener("click", () => removeInstance(instance));

    actions.append(selectButton, editButton, deleteButton);
    row.append(icon, copy, actions);
    elements.instanceList.append(row);
  }
}

function renderSettings() {
  elements.memoryRange.value = String(state.settings.memoryMb);
  elements.memoryValue.textContent = `${state.settings.memoryMb} MB`;
  elements.javaRuntime.value = state.settings.javaRuntime;
  elements.renderer.value = state.settings.renderer;
  elements.keepScreenOn.checked = state.settings.keepScreenOn;
  elements.showLogs.checked = state.settings.showLogs;
  elements.allowCellular.checked = state.settings.allowCellular;
  elements.controllerDeadzone.value = String(state.settings.controllerDeadzone);
  elements.deadzoneValue.textContent = `${Math.round(state.settings.controllerDeadzone * 100)}%`;
  elements.controllerVibration.checked = state.settings.controllerVibration;
  elements.controllerSupportEnabled.checked = state.settings.controllerSupportEnabled;
  elements.launcherFullscreen.checked = state.settings.launcherFullscreen;
  elements.controllerPromptOnStart.checked = state.settings.controllerPromptOnStart;
  elements.hideLauncherWhilePlaying.checked = state.settings.hideLauncherWhilePlaying;
}

function showToast(message, isError = false) {
  const toast = document.createElement("div");
  toast.className = `toast${isError ? " is-error" : ""}`;
  toast.textContent = message;
  elements.toastRegion.append(toast);
  setTimeout(() => toast.remove(), 4200);
}

function errorMessage(error) {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  try { return JSON.stringify(error); } catch { return "Unknown error"; }
}

function openInstanceCreator() {
  elements.instanceName.value = `Ruby Instance ${state.instances.length + 1}`;
  elements.minecraftVersion.value = state.versionCatalog?.latestRelease || "1.21.1";
  elements.loaderType.value = "none";
  elements.instanceDialog.showModal();
  setTimeout(() => markGamepadFocus(elements.instanceName), 40);
}

async function createInstanceFromForm() {
  const request = { name: elements.instanceName.value.trim(), minecraftVersion: elements.minecraftVersion.value.trim(), loader: elements.loaderType.value };
  if (!request.name || !request.minecraftVersion) {
    showToast("An instance name and Minecraft version ID are required.", true);
    return;
  }
  try {
    const instance = await invoke("create_instance", { request });
    state.instances.push(instance);
    elements.instanceDialog.close();
    await persistSelection(instance.id);
    renderInstances();
    showToast(`Created ${instance.name}.`);
    navigate("instances");
  } catch (error) {
    showToast(`Could not create instance: ${errorMessage(error)}`, true);
  }
}

async function removeInstance(instance) {
  const confirmed = window.confirm(`Remove '${instance.name}' from Ruby Launcher?\n\nThis foundation build deletes the instance profile directory.`);
  if (!confirmed) return;
  try {
    await invoke("delete_instance", { instanceId: instance.id });
    state.instances = state.instances.filter((entry) => entry.id !== instance.id);
    const nextSelected = state.selectedInstance?.id === instance.id ? state.instances[0]?.id ?? null : state.selectedInstance?.id;
    await persistSelection(nextSelected);
    renderInstances();
    showToast(`Removed ${instance.name}.`);
  } catch (error) {
    showToast(`Could not remove instance: ${errorMessage(error)}`, true);
  }
}

async function prepareSelectedInstance() {
  if (!state.selectedInstance) return showToast("Create an instance first.", true);
  if (state.selectedInstance.loader !== "none") return showToast("The installer currently supports vanilla profiles only.", true);
  if (state.installRunning) return;

  state.installRunning = true;
  resetInstallDialog();
  elements.installTitle.textContent = `Repairing ${state.selectedInstance.name}`;
  elements.installDialog.showModal();
  elements.closeInstallButton.disabled = true;
  try {
    if (!isTauriRuntime()) {
      updateInstallProgress({ stage: "downloading", message: "Browser preview simulates native downloads.", downloadedBytes: 35, totalBytes: 100, completedFiles: 18, totalFiles: 52, percent: 35 });
      await new Promise((resolve) => setTimeout(resolve, 450));
    }
    const result = await invoke("install_vanilla", { instanceId: state.selectedInstance.id });
    if (!isTauriRuntime()) updateInstallProgress({ stage: "complete", message: result.message, downloadedBytes: 100, totalBytes: 100, completedFiles: 52, totalFiles: 52, percent: 100 });
    state.instances = await invoke("list_instances");
    selectInstanceLocally(state.selectedInstance.id);
    elements.installDetail.textContent = result.message;
    elements.closeInstallButton.disabled = false;
    showToast(result.message);
  } catch (error) {
    updateInstallProgress({ stage: "failed", message: errorMessage(error), downloadedBytes: 0, totalBytes: 0, completedFiles: 0, totalFiles: 0, percent: 0 });
    elements.closeInstallButton.disabled = false;
    showToast(`Installation failed: ${errorMessage(error)}`, true);
    try {
      state.instances = await invoke("list_instances");
      selectInstanceLocally(state.selectedInstance?.id);
    } catch {}
  } finally {
    state.installRunning = false;
  }
}

async function launchSelectedInstance() {
  if (!state.selectedInstance) return showToast("Create an instance first.", true);
  if (!state.account) return showToast("Sign in with the Microsoft account that owns Minecraft first.", true);
  if (state.installRunning) return;

  state.installRunning = true;
  resetInstallDialog();
  elements.installTitle.textContent = `Preparing ${state.selectedInstance.name}`;
  elements.installDetail.textContent = "Play checks required files by location and size, downloads anything missing, prepares Java, then starts Minecraft.";
  elements.installDialog.showModal();
  elements.closeInstallButton.disabled = true;
  try {
    const result = await invoke("play_instance", { instanceId: state.selectedInstance.id });
    if (!isTauriRuntime()) {
      updateInstallProgress({ stage: "complete", message: result.message, downloadedBytes: 1, totalBytes: 1, completedFiles: 1, totalFiles: 1, percent: 100 });
    }
    state.instances = await invoke("list_instances");
    selectInstanceLocally(state.selectedInstance.id);
    renderInstances();
    elements.installDetail.textContent = result.message;
    elements.closeInstallButton.disabled = false;
    showToast(result.message);
  } catch (error) {
    updateInstallProgress({ stage: "failed", message: errorMessage(error), downloadedBytes: 0, totalBytes: 0, completedFiles: 0, totalFiles: 0, percent: 0 });
    elements.closeInstallButton.disabled = false;
    showToast(`Could not play: ${errorMessage(error)}`, true);
    try {
      state.instances = await invoke("list_instances");
      selectInstanceLocally(state.selectedInstance?.id);
      renderInstances();
    } catch {}
  } finally {
    state.installRunning = false;
  }
}

async function saveSettingsFromForm(event) {
  event.preventDefault();
  state.settings = {
    ...state.settings,
    memoryMb: Number(elements.memoryRange.value),
    javaRuntime: elements.javaRuntime.value,
    renderer: elements.renderer.value,
    keepScreenOn: elements.keepScreenOn.checked,
    showLogs: elements.showLogs.checked,
    allowCellular: elements.allowCellular.checked,
    controllerDeadzone: Number(elements.controllerDeadzone.value),
    controllerVibration: elements.controllerVibration.checked,
    controllerSupportEnabled: elements.controllerSupportEnabled.checked,
    launcherFullscreen: elements.launcherFullscreen.checked,
    controllerPromptOnStart: elements.controllerPromptOnStart.checked,
    hideLauncherWhilePlaying: elements.hideLauncherWhilePlaying.checked
  };
  try {
    state.settings = await invoke("save_settings", { settings: state.settings });
    renderSettings();
    applyControllerPromptSetting();
    showToast("Settings saved.");
  } catch (error) {
    showToast(`Could not save settings: ${errorMessage(error)}`, true);
  }
}


function normalizeInstance(instance) {
  return {
    memoryMb: state.settings.memoryMb || 2048,
    initialMemoryMb: 512,
    javaRuntime: state.settings.javaRuntime || "automatic",
    renderer: state.settings.renderer || "automatic",
    jvmPreset: "balanced",
    controllerSupportEnabled: true,
    gameFullscreen: true,
    resolutionWidth: 1280,
    resolutionHeight: 720,
    source: "manual",
    ...instance
  };
}

function jvmPresetArguments(preset) {
  if (preset === "low-memory") return ["-XX:+UseG1GC", "-XX:MaxGCPauseMillis=100"];
  if (preset === "balanced") return ["-XX:+UseG1GC", "-XX:MaxGCPauseMillis=75", "-XX:+ParallelRefProcEnabled"];
  if (preset === "performance") return ["-XX:+UseG1GC", "-XX:MaxGCPauseMillis=50", "-XX:+ParallelRefProcEnabled", "-XX:+AlwaysPreTouch"];
  return [];
}

function updateProfileJvmPreview() {
  const maxMemory = Number(elements.profileMemory.value) || 2048;
  const initialMemory = Math.min(Number(elements.profileInitialMemory.value) || 512, maxMemory);
  elements.profileInitialMemory.max = String(maxMemory);
  if (Number(elements.profileInitialMemory.value) > maxMemory) elements.profileInitialMemory.value = String(maxMemory);
  elements.profileMemoryValue.textContent = `${maxMemory} MB`;
  elements.profileInitialMemoryValue.textContent = `${Math.min(Number(elements.profileInitialMemory.value), maxMemory)} MB`;
  const args = [`-Xms${initialMemory}M`, `-Xmx${maxMemory}M`, ...jvmPresetArguments(elements.profileJvmPreset.value)];
  elements.profileJvmPreview.textContent = args.join(" ");
  const disabled = elements.profileGameFullscreen.checked;
  elements.profileResolutionWidth.disabled = disabled;
  elements.profileResolutionHeight.disabled = disabled;
}

function openProfileEditor(rawInstance) {
  const instance = normalizeInstance(rawInstance);
  elements.profileInstanceId.value = instance.id;
  elements.profileDialogTitle.textContent = `Edit ${instance.name}`;
  elements.profileName.value = instance.name;
  elements.profileMemory.value = String(instance.memoryMb);
  elements.profileInitialMemory.value = String(instance.initialMemoryMb);
  elements.profileJavaRuntime.value = instance.javaRuntime;
  elements.profileRenderer.value = instance.renderer;
  elements.profileJvmPreset.value = instance.jvmPreset;
  elements.profileControllerSupport.checked = instance.controllerSupportEnabled;
  elements.profileGameFullscreen.checked = instance.gameFullscreen;
  elements.profileResolutionWidth.value = String(instance.resolutionWidth);
  elements.profileResolutionHeight.value = String(instance.resolutionHeight);
  updateProfileJvmPreview();
  elements.profileDialog.showModal();
  setTimeout(() => markGamepadFocus(elements.profileName), 40);
}

async function saveProfileFromForm() {
  const request = {
    instanceId: elements.profileInstanceId.value,
    name: elements.profileName.value.trim(),
    memoryMb: Number(elements.profileMemory.value),
    initialMemoryMb: Number(elements.profileInitialMemory.value),
    javaRuntime: elements.profileJavaRuntime.value,
    renderer: elements.profileRenderer.value,
    jvmPreset: elements.profileJvmPreset.value,
    controllerSupportEnabled: elements.profileControllerSupport.checked,
    gameFullscreen: elements.profileGameFullscreen.checked,
    resolutionWidth: Number(elements.profileResolutionWidth.value),
    resolutionHeight: Number(elements.profileResolutionHeight.value)
  };
  try {
    const updated = await invoke("update_instance", { request });
    state.instances = state.instances.map((instance) => instance.id === updated.id ? updated : instance);
    selectInstanceLocally(state.selectedInstance?.id);
    elements.profileDialog.close();
    showToast(`Saved ${updated.name}.`);
  } catch (error) {
    showToast(`Could not save profile: ${errorMessage(error)}`, true);
  }
}

function formatDownloadCount(value) {
  const count = Number(value) || 0;
  return new Intl.NumberFormat(undefined, { notation: count >= 1000 ? "compact" : "standard", maximumFractionDigits: 1 }).format(count);
}

function renderModrinthResults(response) {
  state.modrinth.results = response.hits || [];
  elements.modrinthResults.replaceChildren();
  elements.modrinthEmptyState.hidden = state.modrinth.results.length > 0;
  elements.modrinthStatus.textContent = `${response.totalHits || state.modrinth.results.length} results`;

  for (const project of state.modrinth.results) {
    const card = document.createElement("article");
    card.className = "modrinth-card";
    const icon = document.createElement(project.iconUrl ? "img" : "span");
    icon.className = "modrinth-card-icon";
    if (project.iconUrl) {
      icon.src = project.iconUrl;
      icon.alt = "";
    } else {
      icon.textContent = "M";
    }
    const copy = document.createElement("div");
    copy.className = "modrinth-card-copy";
    const title = document.createElement("h3");
    title.textContent = project.title;
    const meta = document.createElement("span");
    meta.textContent = `by ${project.author} · ${formatDownloadCount(project.downloads)} downloads`;
    const description = document.createElement("p");
    description.textContent = project.description;
    const tags = document.createElement("div");
    tags.className = "tag-row";
    for (const tag of (project.categories || []).slice(0, 4)) {
      const chip = document.createElement("span");
      chip.textContent = tag;
      tags.append(chip);
    }
    copy.append(title, meta, description, tags);
    const button = document.createElement("button");
    button.className = "secondary-button";
    button.type = "button";
    button.textContent = "Choose version";
    button.addEventListener("click", () => openModrinthProject(project));
    card.append(icon, copy, button);
    elements.modrinthResults.append(card);
  }
  requestAnimationFrame(() => focusFirst());
}

async function searchModrinth(event) {
  event?.preventDefault();
  const query = elements.modrinthQuery.value.trim();
  elements.modrinthStatus.textContent = "Searching…";
  elements.modrinthEmptyState.hidden = true;
  try {
    const response = await invoke("search_modrinth_modpacks", { query, index: elements.modrinthSort.value, offset: 0, limit: 24 });
    renderModrinthResults(response);
  } catch (error) {
    elements.modrinthStatus.textContent = "Unavailable";
    elements.modrinthEmptyState.hidden = false;
    showToast(`Could not search Modrinth: ${errorMessage(error)}`, true);
  }
}

function selectedModrinthVersion() {
  return state.modrinth.versions.find((version) => version.id === elements.modrinthVersionSelect.value) || null;
}

function updateModrinthVersionDetails() {
  const version = selectedModrinthVersion();
  if (!version) {
    elements.modrinthVersionDetails.textContent = "No installable version was returned.";
    elements.installModrinthPack.disabled = true;
    return;
  }
  const game = version.gameVersions?.join(", ") || "Unknown Minecraft version";
  const loaders = version.loaders?.join(", ") || "Unknown loader";
  const supported = version.loaders?.some((loader) => ["fabric", "quilt", "minecraft"].includes(loader));
  elements.modrinthVersionDetails.textContent = `Minecraft ${game} · ${loaders} · ${formatDownloadCount(version.downloads)} downloads${supported ? "" : " · Loader installation is not supported in this build"}`;
  elements.installModrinthPack.disabled = !supported;
}

async function openModrinthProject(project) {
  state.modrinth.selectedProject = project;
  elements.modrinthDialogTitle.textContent = project.title;
  elements.modrinthDialogDescription.textContent = project.description;
  elements.modrinthDialogAuthor.textContent = `by ${project.author}`;
  elements.modrinthDialogIcon.hidden = !project.iconUrl;
  if (project.iconUrl) elements.modrinthDialogIcon.src = project.iconUrl;
  elements.modrinthControllerSupport.checked = state.settings.controllerSupportEnabled;
  elements.modrinthVersionSelect.replaceChildren(new Option("Loading versions…", ""));
  elements.installModrinthPack.disabled = true;
  elements.modrinthDialog.showModal();
  try {
    state.modrinth.versions = await invoke("list_modrinth_modpack_versions", { projectId: project.projectId });
    elements.modrinthVersionSelect.replaceChildren();
    for (const version of state.modrinth.versions) {
      const option = new Option(`${version.name} (${version.versionNumber})`, version.id);
      elements.modrinthVersionSelect.append(option);
    }
    updateModrinthVersionDetails();
    setTimeout(() => markGamepadFocus(elements.modrinthVersionSelect), 40);
  } catch (error) {
    elements.modrinthVersionSelect.replaceChildren(new Option("Could not load versions", ""));
    elements.modrinthVersionDetails.textContent = errorMessage(error);
    showToast(`Could not load pack versions: ${errorMessage(error)}`, true);
  }
}

async function installSelectedModrinthPack() {
  const project = state.modrinth.selectedProject;
  const version = selectedModrinthVersion();
  if (!project || !version || state.installRunning) return;
  state.installRunning = true;
  resetInstallDialog();
  elements.installTitle.textContent = `Installing ${project.title}`;
  elements.installDetail.textContent = "Ruby will install the Minecraft base, loader, pack files, and overrides into a separate profile.";
  elements.modrinthDialog.close();
  elements.installDialog.showModal();
  elements.closeInstallButton.disabled = true;
  try {
    const instance = await invoke("install_modrinth_modpack", { request: {
      projectId: project.projectId,
      versionId: version.id,
      name: project.title,
      controllerSupportEnabled: elements.modrinthControllerSupport.checked
    }});
    state.instances = await invoke("list_instances");
    selectInstanceLocally(instance.id);
    elements.closeInstallButton.disabled = false;
    elements.installDetail.textContent = `${instance.name} is ready.`;
    showToast(`Installed ${instance.name}.`);
    navigate("instances", false);
  } catch (error) {
    updateInstallProgress({ stage: "failed", message: errorMessage(error), downloadedBytes: 0, totalBytes: 0, completedFiles: 0, totalFiles: 0, percent: 0 });
    elements.closeInstallButton.disabled = false;
    showToast(`Could not install modpack: ${errorMessage(error)}`, true);
  } finally {
    state.installRunning = false;
  }
}

function formatBytes(value) {
  const bytes = Number(value) || 0;
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB"];
  let size = bytes;
  let index = -1;
  do { size /= 1024; index += 1; } while (size >= 1024 && index < units.length - 1);
  return `${size.toFixed(size >= 100 ? 0 : size >= 10 ? 1 : 2)} ${units[index]}`;
}

function resetInstallDialog() {
  elements.installStage.textContent = "Preparing Mojang metadata…";
  elements.installPercent.textContent = "0%";
  elements.installProgressBar.style.width = "0%";
  elements.installBytes.textContent = "0 B / 0 B";
  elements.installFiles.textContent = "0 / 0 files";
  elements.installDetail.textContent = "Repair verifies Mojang SHA-1 hashes; Play uses a faster location-and-size check.";
}

function updateInstallProgress(progress) {
  const percent = Math.max(0, Math.min(100, Number(progress.percent) || 0));
  elements.installStage.textContent = progress.message || progress.stage || "Working…";
  elements.installPercent.textContent = progress.stage === "failed" ? "Failed" : `${Math.round(percent)}%`;
  elements.installProgressBar.style.width = `${percent}%`;
  elements.installBytes.textContent = `${formatBytes(progress.downloadedBytes)} / ${formatBytes(progress.totalBytes)}`;
  elements.installFiles.textContent = `${progress.completedFiles || 0} / ${progress.totalFiles || 0} files`;
  if (progress.stage === "failed") elements.installDetail.textContent = progress.message;
  if (progress.stage === "complete") {
    elements.installDetail.textContent = progress.message;
    elements.closeInstallButton.disabled = false;
  }
}

async function wireInstallProgressListener() {
  if (!isTauriRuntime() || !window.__TAURI__?.event?.listen) return;
  try {
    await window.__TAURI__.event.listen("install-progress", (event) => updateInstallProgress(event.payload));
    await window.__TAURI__.event.listen("launcher-restored", () => recoverLauncherInput());
    await window.__TAURI__.event.listen("game-exited", async () => {
      recoverLauncherInput();
      try {
        state.instances = (await invoke("list_instances")).map(normalizeInstance);
        selectInstanceLocally(state.selectedInstance?.id);
        renderInstances();
        if (elements.installDialog.open && !state.installRunning) elements.installDialog.close();
        showToast("Minecraft closed. Ruby Launcher is ready again.");
      } catch (error) {
        fallbackLog(`Could not refresh instances after Minecraft exited: ${errorMessage(error)}`);
      }
    });
  } catch (error) {
    fallbackLog(`Could not attach launcher event listeners: ${errorMessage(error)}`);
  }
}

function renderAccount() {
  if (state.account) {
    elements.accountPillName.textContent = state.account.name;
    elements.accountPillStatus.textContent = "Minecraft profile";
    elements.accountTitle.textContent = `Signed in as ${state.account.name}`;
    elements.accountDescription.textContent = state.account.sessionOnly
      ? "Signed in for this launcher session only."
      : "Ruby Launcher will refresh this account using secure device storage.";
    elements.accountSignInButton.textContent = "Sign out";
  } else {
    elements.accountPillName.textContent = "Sign in";
    elements.accountPillStatus.textContent = "Microsoft account";
    elements.accountTitle.textContent = "No Microsoft account connected";
    elements.accountDescription.textContent = "Use Microsoft's device-code page to connect the account that owns Minecraft: Java Edition.";
    elements.accountSignInButton.textContent = "Connect account";
  }
}

function stopAuthPolling() {
  if (state.authPollTimer) clearTimeout(state.authPollTimer);
  state.authPollTimer = null;
}

async function startMicrosoftSignIn() {
  if (state.account) {
    try {
      await invoke("sign_out");
      state.account = null;
      renderAccount();
      showToast("Signed out and removed the saved session from this device.");
    } catch (error) {
      showToast(`Could not sign out: ${errorMessage(error)}`, true);
    }
    return;
  }

  try {
    const result = await invoke("start_microsoft_sign_in");
    state.authVerificationUri = result.verificationUri;
    elements.authUserCode.textContent = result.userCode;
    elements.authVerificationUri.textContent = result.verificationUri;
    elements.authInstructions.textContent = result.message || "Open the Microsoft page and enter this code.";
    elements.authStatus.textContent = "Waiting for you to complete Microsoft sign-in…";
    elements.authDialog.showModal();
    setTimeout(() => markGamepadFocus(elements.openAuthPageButton), 50);
    stopAuthPolling();
    state.authPollTimer = setTimeout(() => pollMicrosoftSignIn(result.interval || 5), (result.interval || 5) * 1000);
  } catch (error) {
    showToast(`Could not start sign-in: ${errorMessage(error)}`, true);
  }
}

async function pollMicrosoftSignIn(intervalSeconds = 5) {
  stopAuthPolling();
  try {
    const result = await invoke("poll_microsoft_sign_in");
    elements.authStatus.textContent = result.message;
    if (result.status === "signedIn") {
      state.account = result.account;
      renderAccount();
      stopAuthPolling();
      if (elements.authDialog.open) elements.authDialog.close();
      elements.rememberDialog.showModal();
      setTimeout(() => markGamepadFocus(elements.rememberYesButton), 60);
      showToast(result.message);
      return;
    }
    if (["expired", "declined"].includes(result.status)) {
      showToast(result.message, true);
      return;
    }
    const retry = result.retryAfter || intervalSeconds;
    state.authPollTimer = setTimeout(() => pollMicrosoftSignIn(retry), retry * 1000);
  } catch (error) {
    elements.authStatus.textContent = errorMessage(error);
    showToast(errorMessage(error), true);
  }
}

async function cancelMicrosoftSignIn() {
  stopAuthPolling();
  try { await invoke("cancel_microsoft_sign_in"); } catch {}
  if (elements.authDialog.open) elements.authDialog.close();
}

async function chooseRememberMe(remember) {
  try {
    state.account = await invoke("set_remember_me", { remember });
    renderAccount();
    if (elements.rememberDialog.open) elements.rememberDialog.close();
    showToast(remember ? "This account will stay signed in on this device." : "This account will be forgotten when Ruby Launcher closes.");
  } catch (error) {
    showToast(`Could not update Remember Me: ${errorMessage(error)}`, true);
  }
}

async function openAuthPage() {
  try {
    await invoke("open_external_url", { url: state.authVerificationUri });
  } catch (error) {
    showToast(`Could not open the browser: ${errorMessage(error)}`, true);
  }
}

function renderVersionCatalog(catalog) {
  state.versionCatalog = catalog;
  elements.latestReleaseValue.textContent = catalog.latestRelease;
  elements.latestSnapshotValue.textContent = catalog.latestSnapshot;
  elements.catalogStatus.textContent = `${catalog.versions.length} recent entries`;
  elements.catalogList.replaceChildren();
  elements.versionOptions.replaceChildren();
  for (const version of catalog.versions) {
    const card = document.createElement("article");
    card.className = "catalog-version";
    const name = document.createElement("strong");
    name.textContent = version.id;
    const type = document.createElement("span");
    type.textContent = version.type;
    card.append(name, type);
    elements.catalogList.append(card);

    const option = document.createElement("option");
    option.value = version.id;
    elements.versionOptions.append(option);
  }
}

async function refreshVersionCatalog() {
  elements.catalogStatus.textContent = "Connecting…";
  try {
    renderVersionCatalog(await invoke("fetch_version_catalog"));
  } catch (error) {
    elements.catalogStatus.textContent = "Unavailable";
    elements.latestReleaseValue.textContent = "—";
    elements.latestSnapshotValue.textContent = "—";
    showToast(`Could not load Mojang's catalog: ${errorMessage(error)}`, true);
  }
}

async function refreshLogs() {
  elements.logOutput.textContent = "Loading launcher log…";
  try {
    const logs = await invoke("read_launcher_log");
    elements.logOutput.textContent = logs || "No log entries yet.";
    elements.logOutput.scrollTop = elements.logOutput.scrollHeight;
  } catch (error) {
    elements.logOutput.textContent = `Could not read log:\n${errorMessage(error)}`;
  }
}

function closeLauncherMenu() {
  if (elements.launcherMenuDialog.open) elements.launcherMenuDialog.close();
}

function openLauncherMenu() {
  if (elements.launcherMenuDialog.open) {
    closeLauncherMenu();
    return;
  }
  elements.launcherMenuDialog.showModal();
  setTimeout(() => markGamepadFocus(document.querySelector("#openGameLibraryButton")), 40);
  pulseController(28, 0.22);
}

function openGameLibraryPlaceholder() {
  closeLauncherMenu();
  elements.gameLibraryDialog.showModal();
  setTimeout(() => markGamepadFocus(document.querySelector("#closeGameLibraryButton")), 40);
}

function openSettingsFromLauncherMenu() {
  closeLauncherMenu();
  navigate("settings");
}

async function exitLauncher() {
  closeLauncherMenu();
  try {
    await invoke("exit_launcher");
    if (!isTauriRuntime()) showToast("Exit is only available in the desktop launcher build.");
  } catch (error) {
    showToast(`Could not exit Ruby Launcher: ${errorMessage(error)}`, true);
  }
}

async function openLogs() {
  closeLauncherMenu();
  if (elements.logsDialog.open) {
    await refreshLogs();
    return;
  }
  elements.logsDialog.showModal();
  await refreshLogs();
  setTimeout(() => markGamepadFocus(document.querySelector("#closeLogsButton")), 40);
}

async function seedFirstInstance() {
  if (state.instances.length > 0) return;
  try {
    const instance = await invoke("create_instance", { request: { name: "Ruby Test Instance", minecraftVersion: "1.21.1", loader: "none" } });
    state.instances = [instance];
    state.settings.selectedInstanceId = instance.id;
    await invoke("save_settings", { settings: state.settings });
  } catch (error) {
    showToast(`Could not create the starter instance: ${errorMessage(error)}`, true);
  }
}

function wireEvents() {
  for (const button of elements.navButtons) button.addEventListener("click", () => navigate(button.dataset.viewTarget));
  for (const button of document.querySelectorAll("[data-navigate]")) button.addEventListener("click", () => navigate(button.dataset.navigate));

  document.querySelector("#createInstanceButton").addEventListener("click", openInstanceCreator);
  document.querySelector("#emptyCreateButton").addEventListener("click", openInstanceCreator);
  document.querySelector("#manualProfileButton").addEventListener("click", openInstanceCreator);
  elements.instanceForm.addEventListener("submit", (event) => {
    event.preventDefault();
    if (event.submitter?.value === "cancel") {
      elements.instanceDialog.close();
      requestAnimationFrame(() => focusFirst());
      return;
    }
    createInstanceFromForm();
  });
  elements.profileForm.addEventListener("submit", (event) => {
    event.preventDefault();
    if (event.submitter?.value === "cancel") {
      elements.profileDialog.close();
      return;
    }
    saveProfileFromForm();
  });
  for (const input of [elements.profileMemory, elements.profileInitialMemory, elements.profileJvmPreset, elements.profileGameFullscreen]) {
    input.addEventListener("input", updateProfileJvmPreview);
    input.addEventListener("change", updateProfileJvmPreview);
  }

  elements.modrinthSearchForm.addEventListener("submit", searchModrinth);
  elements.modrinthVersionSelect.addEventListener("change", updateModrinthVersionDetails);
  elements.installModrinthPack.addEventListener("click", installSelectedModrinthPack);
  document.querySelector("#closeModrinthDialog").addEventListener("click", () => elements.modrinthDialog.close());
  document.querySelector("#cancelModrinthInstall").addEventListener("click", () => elements.modrinthDialog.close());

  document.querySelector("#prepareButton").addEventListener("click", prepareSelectedInstance);
  document.querySelector("#playButton").addEventListener("click", launchSelectedInstance);
  document.querySelector("#signInButton").addEventListener("click", startMicrosoftSignIn);
  elements.accountSignInButton.addEventListener("click", startMicrosoftSignIn);
  document.querySelector("#instanceMenuButton").addEventListener("click", () => {
    if (state.selectedInstance) openProfileEditor(state.selectedInstance);
  });
  document.querySelector("#refreshCatalogButton").addEventListener("click", refreshVersionCatalog);
  elements.rememberYesButton.addEventListener("click", () => chooseRememberMe(true));
  elements.rememberNoButton.addEventListener("click", () => chooseRememberMe(false));
  document.querySelector("#openAuthPageButton").addEventListener("click", openAuthPage);
  document.querySelector("#cancelAuthButton").addEventListener("click", cancelMicrosoftSignIn);
  document.querySelector("#closeAuthButton").addEventListener("click", cancelMicrosoftSignIn);
  elements.closeInstallButton.addEventListener("click", () => { if (!state.installRunning) elements.installDialog.close(); });

  elements.memoryRange.addEventListener("input", () => { elements.memoryValue.textContent = `${elements.memoryRange.value} MB`; });
  elements.controllerDeadzone.addEventListener("input", () => { elements.deadzoneValue.textContent = `${Math.round(Number(elements.controllerDeadzone.value) * 100)}%`; });
  elements.settingsForm.addEventListener("submit", saveSettingsFromForm);
  elements.testVibrationButton.addEventListener("click", async () => {
    await pulseController(220, 0.5);
    showToast("Vibration test sent. Some browsers and controllers do not expose rumble.");
  });

  document.querySelector("#openGameLibraryButton").addEventListener("click", openGameLibraryPlaceholder);
  document.querySelector("#openLauncherSettingsButton").addEventListener("click", openSettingsFromLauncherMenu);
  document.querySelector("#exitLauncherButton").addEventListener("click", exitLauncher);
  document.querySelector("#closeLauncherMenuButton").addEventListener("click", closeLauncherMenu);
  document.querySelector("#closeGameLibraryButton").addEventListener("click", () => elements.gameLibraryDialog.close());

  document.querySelector("#openLogsButton").addEventListener("click", openLogs);
  document.querySelector("#closeLogsButton").addEventListener("click", () => { elements.logsDialog.close(); requestAnimationFrame(() => focusFirst()); });
  document.querySelector("#refreshLogsButton").addEventListener("click", refreshLogs);

  for (const dialog of document.querySelectorAll("dialog")) {
    dialog.addEventListener("close", () => requestAnimationFrame(() => focusFirst()));
  }

  window.addEventListener("gamepadconnected", (event) => unlockForController(event.gamepad));
  window.addEventListener("gamepaddisconnected", (event) => {
    if (event.gamepad.index === state.controller.index) lockForController();
  });

  document.addEventListener("keydown", (event) => {
    if (state.settings.controllerPromptOnStart && !state.controller.connected) return;
    const tag = document.activeElement?.tagName;
    const editingText = tag === "INPUT" && !["range", "checkbox", "radio"].includes(document.activeElement.type);
    if (editingText && !["Escape"].includes(event.key)) return;

    const directions = { ArrowLeft: "left", ArrowRight: "right", ArrowUp: "up", ArrowDown: "down" };
    if (directions[event.key]) {
      event.preventDefault();
      navigateDirection(directions[event.key]);
    } else if (event.key === "Enter" && !(document.activeElement instanceof HTMLButtonElement)) {
      event.preventDefault();
      activateFocused();
    } else if (event.key === "F10") {
      event.preventDefault();
      openLauncherMenu();
    } else if (event.key === "F9") {
      event.preventDefault();
      openLogs();
    } else if (event.key === "Escape" || event.key === "Backspace") {
      event.preventDefault();
      handleBack();
    } else if (["1", "2", "3", "4", "5"].includes(event.key)) {
      navigate(VIEW_ORDER[Number(event.key) - 1]);
    }
  });
}

async function initialize() {
  wireEvents();
  try {
    const [launcherInfo, settings, instances] = await Promise.all([invoke("launcher_info"), invoke("load_settings"), invoke("list_instances")]);
    state.launcherInfo = launcherInfo;
    state.settings = { ...DEFAULT_SETTINGS, ...settings };
    state.instances = instances.map(normalizeInstance);
    try {
      state.account = await invoke("restore_saved_account");
    } catch (error) {
      state.account = null;
      showToast(`Saved account could not be restored: ${errorMessage(error)}`, true);
    }
    await seedFirstInstance();
    selectInstanceLocally(state.settings.selectedInstanceId ?? state.instances[0]?.id);
    renderSettings();
    renderInstances();
    renderAccount();
    wireInstallProgressListener();
    refreshVersionCatalog();
    elements.modrinthSort.value = "downloads";
    searchModrinth();
  } catch (error) {
    showToast(`Ruby Launcher could not initialize: ${errorMessage(error)}`, true);
  }

  applyControllerPromptSetting();
  requestAnimationFrame(pollController);
}

initialize();
