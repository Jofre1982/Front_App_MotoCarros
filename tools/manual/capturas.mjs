// Recorre la app como pasajera y conductor y guarda pantallazos para el manual.
import { chromium } from "playwright";
import fs from "node:fs";

const APP = "http://localhost:8080/";
const OUT = "capturas";
fs.mkdirSync(OUT, { recursive: true });
const INIRIDA = { latitude: 3.8653, longitude: -67.9239 };
const log = (...a) => console.log(new Date().toISOString().slice(11, 19), ...a);

const browser = await chromium.launch();
async function phone() {
  const ctx = await browser.newContext({
    viewport: { width: 390, height: 844 },
    deviceScaleFactor: 2,
    isMobile: true,
    hasTouch: true,
    locale: "es-CO",
    geolocation: INIRIDA,
    permissions: ["geolocation"],
  });
  const page = await ctx.newPage();
  page.on("console", (m) => { if (m.type() === "error") log("console:", m.text().slice(0, 200)); });
  page.on("pageerror", (e) => log("pageerror:", String(e).slice(0, 200)));
  return page;
}

async function shot(page, name, full = true) {
  await page.waitForTimeout(900);
  await page.screenshot({ path: `${OUT}/${name}.png`, fullPage: full });
  log("📸", name);
}

async function step(name, fn) {
  try { await fn(); } catch (e) { log("✗", name, String(e).split("\n")[0]); }
}

async function login(page, email, prefix) {
  await page.goto(APP);
  await page.waitForSelector("#login-email", { timeout: 60000 });
  await shot(page, `${prefix}_01_ingreso`, false);
  await page.fill("#login-email", email);
  await page.fill("#login-password", "demo12345");
  await shot(page, `${prefix}_02_ingreso_lleno`, false);
  await page.getByRole("button", { name: "Ingresar", exact: true }).click();
  await page.waitForSelector("nav.home-nav", { timeout: 30000 });
}

const P = await phone();
const D = await phone();

await step("registro", async () => {
  const R = await phone();
  await R.goto(APP);
  await R.waitForSelector("#login-email", { timeout: 60000 });
  await R.getByText("¿Usuario nuevo? Crear cuenta").click();
  await shot(R, "a_00_registro");
});

await step("login pasajera", async () => {
  await login(P, "pasajera@demo.co", "p");
  await P.waitForTimeout(4000);
  await shot(P, "p_03_inicio", false);
  await shot(P, "p_03_inicio_full");
});

await step("pedir", async () => {
  await P.fill("#ride-pickup-only-destination", "Al hospital");
  await shot(P, "p_04_pedir_lleno");
  await P.getByRole("button", { name: "Pedir motocarro", exact: true }).last().click();
  await P.waitForTimeout(3000);
  await shot(P, "p_05_esperando");
  await shot(P, "p_05_esperando_vista", false);
});

await step("login conductor", async () => {
  await login(D, "conductor@demo.co", "c");
  await D.waitForTimeout(4000);
  await shot(D, "c_03_inicio");
});

await step("encender", async () => {
  const btn = D.getByRole("button", { name: /Encender/ });
  if (await btn.count()) {
    await btn.first().click();
    await D.waitForTimeout(4000);
  }
  await shot(D, "c_04_encendido");
});

await step("solicitud", async () => {
  await D.waitForSelector("li.nearby-ride-request", { timeout: 60000 });
  await shot(D, "c_05_solicitud");
  await D.locator("li.nearby-ride-request input[type=number]").first().fill("5000");
  await shot(D, "c_06_precio");
  await D.locator("li.nearby-ride-request .nearby-ride-accept-button").first().click();
  await D.waitForTimeout(3000);
  await shot(D, "c_07_esperando_respuesta");
});

await step("oferta", async () => {
  await P.waitForSelector(".price-offer", { timeout: 60000 });
  await shot(P, "p_06_oferta");
  await shot(P, "p_06_oferta_vista", false);
  await P.locator(".price-offer button").first().click();
  await P.waitForTimeout(5000);
  await shot(P, "p_07_aceptado");
  await shot(P, "p_07_aceptado_vista", false);
});

await step("conductor viaje", async () => {
  await D.waitForTimeout(8000);
  await shot(D, "c_08_aceptado");
  await D.getByRole("button", { name: "Iniciar viaje" }).first().click();
  await D.waitForTimeout(3000);
  await shot(D, "c_09_en_viaje");
  await D.getByRole("button", { name: "Completar viaje" }).first().click();
  await D.waitForTimeout(3000);
  await shot(D, "c_10_completado");
});

await step("pasajera fin", async () => {
  await P.waitForTimeout(8000);
  await shot(P, "p_08_terminado");
});

await step("mis viajes", async () => {
  await P.getByRole("button", { name: "Mis viajes", exact: true }).click();
  await P.waitForTimeout(4000);
  await shot(P, "p_09_mis_viajes");
  await P.locator(".favorite-toggle").first().click();
  await P.waitForTimeout(2500);
  await shot(P, "p_10_favorito");
});

await step("favorito al pedir", async () => {
  await P.getByRole("button", { name: "Pedir motocarro", exact: true }).first().click();
  await P.waitForTimeout(4000);
  const picker = P.locator(".favorite-picker");
  await picker.scrollIntoViewIfNeeded();
  await shot(P, "p_11_pedir_con_favorito");
  await picker.screenshot({ path: `${OUT}/p_11b_selector_favorito.png` });
});

await step("mandado", async () => {
  await P.getByText("🛍️ Mandado").click();
  await P.waitForTimeout(2500);
  await shot(P, "p_12_mandado");
});

await step("motocarga", async () => {
  await P.getByRole("button", { name: "Pedir motocarga", exact: true }).click();
  await P.waitForTimeout(3000);
  await shot(P, "p_13_motocarga");
});

await step("perfil pasajera", async () => {
  await P.getByRole("button", { name: "Mi perfil", exact: true }).click();
  await P.waitForTimeout(3000);
  await shot(P, "p_14_perfil");
});

for (const [label, name] of [
  ["Historial y ganancias", "c_11_ganancias"],
  ["Mi vehiculo", "c_12_vehiculo"],
  ["Mis documentos", "c_13_documentos"],
  ["Mandados cercanos", "c_14_mandados"],
  ["Mi perfil", "c_15_perfil"],
]) {
  await step(label, async () => {
    await D.getByRole("button", { name: label, exact: true }).click();
    await D.waitForTimeout(3500);
    await shot(D, name);
  });
}

await browser.close();
log("listo");
