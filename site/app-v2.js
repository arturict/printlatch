// Versioned asset: bump the filename whenever this file changes.
const menuButton = document.querySelector(".menu-button");
const navLinks = document.querySelector(".nav-links");
const siteHeader = document.querySelector(".site-header");
const demoTabs = document.querySelectorAll(".demo-tab");
const demoPanels = document.querySelectorAll(".demo-panel");

menuButton?.addEventListener("click", () => {
  const open = menuButton.getAttribute("aria-expanded") !== "true";
  menuButton.setAttribute("aria-expanded", String(open));
  navLinks?.classList.toggle("open", open);
});

navLinks?.addEventListener("click", (event) => {
  if (event.target instanceof HTMLAnchorElement) {
    menuButton?.setAttribute("aria-expanded", "false");
    navLinks.classList.remove("open");
  }
});

demoTabs.forEach((tab) => {
  tab.addEventListener("click", () => {
    const panel = tab.getAttribute("data-panel");

    demoTabs.forEach((candidate) => {
      const active = candidate === tab;
      candidate.classList.toggle("active", active);
      candidate.setAttribute("aria-pressed", String(active));
    });

    demoPanels.forEach((candidate) => {
      candidate.classList.toggle("active", candidate.getAttribute("data-panel-content") === panel);
    });
  });
});

const updateHeader = () => {
  siteHeader?.classList.toggle("scrolled", window.scrollY > 12);
};

updateHeader();
window.addEventListener("scroll", updateHeader, { passive: true });
