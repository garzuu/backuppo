import { test, expect } from "@playwright/test";

test.describe("Dashboard agent", () => {
  test("mostra la panoramica e naviga tra le sezioni principali", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByRole("heading", { name: "Panoramica" })).toBeVisible();

    await page.getByRole("button", { name: "Storico" }).click();
    await expect(page.getByRole("heading", { name: "Storico esecuzioni" })).toBeVisible();

    await page.getByRole("button", { name: "Configurazione" }).click();
    await expect(page.getByRole("heading", { name: "Configurazione backup" })).toBeVisible();
  });
});

test.describe("Wizard di configurazione", () => {
  test("crea un job, aggiunge un notifier e valida la configurazione", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Configurazione" }).click();
    await expect(page.getByRole("heading", { name: "Configurazione backup" })).toBeVisible();

    // La prima esecuzione (qualunque progetto/viewport parta per primo)
    // trova la configurazione vuota; le successive trovano già un job.
    const emptyState = page.locator(".wizard-empty");
    const sidebar = page.locator(".job-sidebar");
    await expect(emptyState.or(sidebar)).toBeVisible();
    if (await emptyState.isVisible()) {
      await page.getByRole("button", { name: "Crea il primo job" }).click();
    } else {
      await sidebar.getByTitle("Aggiungi un job").click();
    }

    await expect(page.locator(".job-sidebar")).toContainText("nuovo-backup");
    await expect(page.getByRole("heading", { name: "Cosa vuoi salvare?" })).toBeVisible();

    // Sorgente e destinazione: i default (cartella locale) sono già validi.
    await page.getByRole("button", { name: "Continua" }).click();
    await expect(page.getByRole("heading", { name: "Dove vuoi conservare il backup?" })).toBeVisible();
    await page.getByRole("button", { name: "Continua" }).click();
    await expect(page.getByRole("heading", { name: "Quando deve partire?" })).toBeVisible();
    await page.getByRole("button", { name: "Continua" }).click();

    // Protezione: crea un notifier webhook e lo attiva per i fallimenti.
    await expect(page.getByRole("heading", { name: "Protezione e controlli" })).toBeVisible();
    await page.getByPlaceholder("Nome nuovo notifier").fill("e2e-webhook");
    await page.getByRole("button", { name: "Aggiungi notifier" }).click();

    const notifierItem = page.locator(".notifier-manage-item", { hasText: "e2e-webhook" });
    await expect(notifierItem).toBeVisible();
    await notifierItem.getByLabel("URL webhook").fill("https://hooks.example.com/e2e");

    const notifierRow = page.locator(".notifier-grid > label", { hasText: "e2e-webhook" });
    await notifierRow.locator("input[type=checkbox]").first().check();

    await page.getByRole("button", { name: "Continua" }).click();
    await expect(page.getByRole("heading", { name: "Controlla il job" })).toBeVisible();
    await expect(page.locator(".review-list")).toContainText("nuovo-backup");

    await page.getByRole("button", { name: "Valida" }).click();
    await expect(page.locator(".notice")).toContainText("Configurazione valida");
  });
});
