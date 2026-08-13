const SUPPORTED_SYSTEMS = new Set(["macos", "windows"]);

/**
 * Detect only the desktop operating systems for which MineTrace ships.
 * iPadOS is checked first because it can identify its platform as MacIntel.
 */
export function detectOperatingSystem({
  platform = "",
  userAgent = "",
  maxTouchPoints = 0,
} = {}) {
  const platformName = String(platform).toLowerCase();
  const agent = String(userAgent).toLowerCase();

  const isAppleMobile =
    /iphone|ipad|ipod/.test(agent) ||
    (platformName.includes("mac") && Number(maxTouchPoints) > 1);

  if (isAppleMobile || agent.includes("windows phone")) {
    return "unsupported";
  }

  if (platformName.includes("win") || agent.includes("windows")) {
    return "windows";
  }

  if (
    platformName.includes("mac") ||
    agent.includes("macintosh") ||
    agent.includes("mac os x")
  ) {
    return "macos";
  }

  return "unsupported";
}

export function showDownloadForOperatingSystem(documentRoot, operatingSystem) {
  const downloadOptions = documentRoot.getElementById("download-options");
  const unsupportedMessage = documentRoot.getElementById("unsupported-platform");
  const cards = documentRoot.querySelectorAll("[data-download-platform]");

  if (!downloadOptions || !unsupportedMessage || cards.length === 0) {
    return;
  }

  const isSupported = SUPPORTED_SYSTEMS.has(operatingSystem);
  downloadOptions.hidden = !isSupported;
  unsupportedMessage.hidden = isSupported;

  for (const card of cards) {
    const isMatch = isSupported && card.dataset.downloadPlatform === operatingSystem;
    card.classList.toggle("is-recommended", isMatch);

    const matchLabel = card.querySelector("[data-device-match]");
    if (matchLabel) {
      matchLabel.hidden = !isMatch;
    }
  }
}

if (typeof document !== "undefined" && typeof navigator !== "undefined") {
  const operatingSystem = detectOperatingSystem({
    platform: navigator.userAgentData?.platform || navigator.platform,
    userAgent: navigator.userAgent,
    maxTouchPoints: navigator.maxTouchPoints,
  });

  showDownloadForOperatingSystem(document, operatingSystem);
}
