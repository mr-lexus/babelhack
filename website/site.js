const reducedMotion = matchMedia("(prefers-reduced-motion: reduce)");
const motionToggle = document.getElementById("motion-toggle");
const motionLabel = document.getElementById("motion-label");
const motionStorageKey = "babelhack-site-motion";
let motionPaused = false;
try {
  motionPaused = localStorage.getItem(motionStorageKey) === "paused";
} catch {
  /* Storage is optional. */
}
const activeAnimations = new Map();
const motionAllowed = () =>
  !reducedMotion.matches && !motionPaused && !document.hidden;

function animateIn(element, { delay = 0, distance = 18, duration = 650 } = {}) {
  if (!motionAllowed() || !element?.animate) return;
  activeAnimations.get(element)?.cancel();
  const animation = element.animate(
    [
      { opacity: 0, transform: `translateY(${distance}px)` },
      { opacity: 1, transform: "translateY(0)" },
    ],
    { duration, delay, easing: "cubic-bezier(.16,1,.3,1)", fill: "backwards" },
  );
  activeAnimations.set(element, animation);
  const clear = () => {
    if (activeAnimations.get(element) === animation)
      activeAnimations.delete(element);
  };
  animation.onfinish = clear;
  animation.oncancel = clear;
}
function syncMotion() {
  document.documentElement.dataset.motion = motionAllowed() ? "on" : "off";
  motionToggle.setAttribute(
    "aria-pressed",
    String(!motionPaused && !reducedMotion.matches),
  );
  motionToggle.disabled = reducedMotion.matches;
  motionLabel.textContent = reducedMotion.matches
    ? "Без анимации — настройка системы"
    : motionPaused
      ? "Анимация выключена"
      : "Анимация включена";
  if (!motionAllowed()) {
    activeAnimations.forEach((animation) => animation.cancel());
    activeAnimations.clear();
  }
}
motionToggle.hidden = false;
motionToggle.addEventListener("click", () => {
  motionPaused = !motionPaused;
  try {
    localStorage.setItem(motionStorageKey, motionPaused ? "paused" : "enabled");
  } catch {
    /* Keep the in-memory preference. */
  }
  syncMotion();
});
reducedMotion.addEventListener("change", syncMotion);
document.addEventListener("visibilitychange", syncMotion);
window.addEventListener("focus", syncMotion);
window.addEventListener("pageshow", syncMotion);
// The CSS media query also stops ambient animations. Use that cancellation
// as a second signal when a rapid preference change is coalesced by the browser.
document.addEventListener("animationcancel", () => {
  if (reducedMotion.matches) syncMotion();
});
syncMotion();

// Content stays visible without JS or an observer. Nothing waits for animation to become usable.
if ("IntersectionObserver" in window) {
  const revealObserver = new IntersectionObserver(
    (entries) => {
      entries.forEach((entry) => {
        if (!entry.isIntersecting) return;
        revealObserver.unobserve(entry.target);
        animateIn(entry.target, {
          delay: Number(entry.target.dataset.revealDelay || 0),
        });
      });
    },
    { threshold: 0.08 },
  );
  document
    .querySelectorAll(
      ".section-heading, .feature-grid article, .workspace, .detail-row > p, .steps > li, .cost-note, .transparency > div, .download-copy, .download-box, .faq > div, .footer-top",
    )
    .forEach((element) => {
      if (element.matches(".feature-grid article, .steps > li"))
        element.dataset.revealDelay = String(
          [...element.parentElement.children].indexOf(element) * 90,
        );
      revealObserver.observe(element);
    });
  const ambientObserver = new IntersectionObserver((entries) => {
    entries.forEach((entry) =>
      entry.target.classList.toggle("ambient-visible", entry.isIntersecting),
    );
  });
  document
    .querySelectorAll(".hero, .download-section")
    .forEach((element) => ambientObserver.observe(element));
}
document
  .querySelectorAll(
    ".hero > .eyebrow, #hero-title, .hero-description, .hero-figure",
  )
  .forEach((element, index) =>
    animateIn(element, { delay: index * 90, distance: 22, duration: 850 }),
  );
document.addEventListener("focusin", (event) => {
  // A keyboard user should never land on a faded or moving control.
  activeAnimations.forEach((animation, element) => {
    if (element.contains(event.target)) animation.cancel();
  });
});
document.querySelectorAll(".faq details").forEach((detail) =>
  detail.addEventListener("toggle", () => {
    if (detail.open)
      animateIn(detail.querySelector("p"), { distance: 6, duration: 280 });
  }),
);

const releaseBase =
  "https://github.com/mr-lexus/babelhack/releases/download/v0.7.0/BabelHack-0.7.0-";
const platformTabs = [...document.querySelectorAll("[data-platform]")];
function selectPlatform(platform, focus = false) {
  platformTabs.forEach((tab) => {
    const selected = tab.dataset.platform === platform;
    tab.setAttribute("aria-selected", String(selected));
    tab.tabIndex = selected ? 0 : -1;
    const panel = document.getElementById(tab.getAttribute("aria-controls"));
    const wasHidden = panel.hidden;
    panel.hidden = !selected;
    if (selected && wasHidden) animateIn(panel, { distance: 8, duration: 300 });
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
    description: "Здесь видны оригинал, перевод и уже законченные фразы.",
    caption:
      "Живой перевод · Babel Hack 0.7.0 · Windows · демонстрационная сессия",
  },
  settings: {
    image: "assets/app-settings.png",
    alt: "Настройки Babel Hack: подключения, языки и оформление окна субтитров",
    description:
      "Выберите языки, добавьте ключи и настройте, как будут выглядеть субтитры.",
    caption:
      "Настройки · Babel Hack 0.7.0 · Windows · ключи не отображаются в интерфейсе",
  },
};
let galleryRevision = 0;
document.querySelectorAll("[data-gallery]").forEach((button) =>
  button.addEventListener("click", () => {
    const item = gallery[button.dataset.gallery];
    const image = document.getElementById("workspace-image");
    if (image.getAttribute("src") === item.image) return;
    const revision = ++galleryRevision;
    activeAnimations.get(image)?.cancel();
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
    // Ignore late image decodes after another tab was selected.
    image
      .decode()
      .then(() => {
        if (revision === galleryRevision)
          animateIn(image, { distance: 10, duration: 400 });
      })
      .catch(() => {});
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
    animateIn(dialog, { distance: 10, duration: 260 });
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
