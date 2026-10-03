const releaseBase =
  "https://github.com/mr-lexus/babelhack/releases/download/v0.7.0/BabelHack-0.7.0-";
const platformTabs = [...document.querySelectorAll("[data-platform]")];
function selectPlatform(platform, focus = false) {
  platformTabs.forEach((tab) => {
    const selected = tab.dataset.platform === platform;
    tab.setAttribute("aria-selected", String(selected));
    tab.tabIndex = selected ? 0 : -1;
    document.getElementById(tab.getAttribute("aria-controls")).hidden =
      !selected;
    if (selected && focus) tab.focus();
  });
}
platformTabs.forEach((tab, index) => {
  tab.addEventListener("click", () => selectPlatform(tab.dataset.platform));
  tab.addEventListener("keydown", (event) => {
    let target;
    if (event.key === "ArrowRight") target = (index + 1) % platformTabs.length;
    if (event.key === "ArrowLeft")
      target = (index + platformTabs.length - 1) % platformTabs.length;
    if (event.key === "Home") target = 0;
    if (event.key === "End") target = platformTabs.length - 1;
    if (target !== undefined) {
      event.preventDefault();
      selectPlatform(platformTabs[target].dataset.platform, true);
    }
  });
});
// Detect only the OS family, never guess CPU architecture from the browser.
const userAgent = navigator.userAgent;
const mobile =
  /Android|iPhone|iPad|iPod/.test(userAgent) ||
  (navigator.platform === "MacIntel" && navigator.maxTouchPoints > 1);
selectPlatform(
  !mobile && /Macintosh|Mac OS X/.test(userAgent)
    ? "macos"
    : !mobile && /Linux/.test(userAgent)
      ? "linux"
      : "windows",
);
function setDownload(id, asset, label) {
  const link = document.getElementById(id);
  link.href = releaseBase + asset;
  link.dataset.asset = asset;
  if (label) {
    link.replaceChildren(document.createTextNode(label + " "));
    const arrow = document.createElement("span");
    arrow.setAttribute("aria-hidden", "true");
    arrow.textContent = "↓";
    link.append(arrow);
  }
}
document.getElementById("mac-arch").addEventListener("change", (event) => {
  const arch = event.target.value;
  setDownload(
    "mac-download",
    `macos-${arch}.dmg`,
    `Скачать .dmg · ${arch === "arm64" ? "Apple Silicon" : "Intel"}`,
  );
  setDownload("mac-zip", `macos-${arch}.app.zip`);
});
function updateLinux() {
  const arch = document.getElementById("linux-arch").value;
  const format = document.getElementById("linux-format").value;
  setDownload(
    "linux-download",
    `linux-${arch}.${format}`,
    `Скачать .${format} · ${arch === "arm64" ? "ARM64" : "x64"}`,
  );
}
document.getElementById("linux-arch").addEventListener("change", updateLinux);
document.getElementById("linux-format").addEventListener("change", updateLinux);

const gallery = {
  session: {
    image: "assets/app-session.png",
    alt: "Главное окно Babel Hack: оригинал, перевод и лента разговора в демонстрационной сессии",
    description:
      "Оригинал, перевод и лента разговора — в одном рабочем пространстве.",
    caption:
      "Живой перевод · Babel Hack 0.7.0 · Windows · демонстрационная сессия",
  },
  settings: {
    image: "assets/app-settings.png",
    alt: "Настройки Babel Hack: подключения, языки и оформление окна субтитров",
    description:
      "Подключения, языки, словарь и оформление — под ваш рабочий процесс.",
    caption:
      "Настройки · Babel Hack 0.7.0 · Windows · тестовый профиль без API-ключей",
  },
};
document.querySelectorAll("[data-gallery]").forEach((button) =>
  button.addEventListener("click", () => {
    const item = gallery[button.dataset.gallery];
    const image = document.getElementById("workspace-image");
    image.src = item.image;
    image.alt = item.alt;
    image.parentElement.dataset.lightbox = item.image;
    image.parentElement.dataset.caption = item.caption;
    document.getElementById("gallery-description").textContent =
      item.description;
    document.querySelectorAll("[data-gallery]").forEach((other) => {
      const active = button === other;
      other.setAttribute("aria-pressed", String(active));
      other.classList.toggle("active", active);
    });
  }),
);
const dialog = document.getElementById("screenshot-dialog");
let lightboxTrigger;
document.querySelectorAll("[data-lightbox]").forEach((button) =>
  button.addEventListener("click", () => {
    lightboxTrigger = button;
    const image = document.getElementById("screenshot-full");
    image.src = button.dataset.lightbox;
    image.alt = button.querySelector("img").alt;
    document.getElementById("screenshot-caption").textContent =
      button.dataset.caption;
    dialog.showModal();
  }),
);
document
  .querySelector(".dialog-close")
  .addEventListener("click", () => dialog.close());
dialog.addEventListener("click", (event) => {
  if (event.target === dialog) {
    const rect = dialog.getBoundingClientRect();
    if (
      event.clientX < rect.left ||
      event.clientX > rect.right ||
      event.clientY < rect.top ||
      event.clientY > rect.bottom
    )
      dialog.close();
  }
});
dialog.addEventListener("close", () =>
  lightboxTrigger?.focus({ preventScroll: true }),
);
