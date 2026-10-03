import { expect, test } from "@playwright/test";
import { installBackend } from "./mock-backend";

for (const theme of ["light", "dark"] as const) {
  test(`${theme}: transfer icons and theme choices stay visually distinct`, async ({ page }) => {
    await installBackend(page, theme);
    await page.goto("/");
    const transfer = page.locator(".workspace-header .transfer-action");
    await expect(transfer).toHaveCount(2);
    await expect(transfer.nth(0).locator("svg.lucide-arrow-down-to-line")).toBeVisible();
    await expect(transfer.nth(1).locator("svg.lucide-arrow-up-from-line")).toBeVisible();
    await page.locator(".sidebar").getByRole("button", { name: "设置", exact: true }).click();
    const light = page.locator(".preference-option-light");
    const dark = page.locator(".preference-option-dark");
    await expect(light).toBeVisible();
    await expect(dark).toBeVisible();
    const colors = await page.evaluate(() => {
      const color = (selector: string) => {
        const element = document.querySelector(selector)!;
        const style = getComputedStyle(element);
        return { background: style.backgroundColor, text: getComputedStyle(element.querySelector(".preference-option-head")!).color };
      };
      return { light: color(".preference-option-light"), dark: color(".preference-option-dark") };
    });
    expect(colors.light).toEqual({ background: "rgb(255, 255, 255)", text: "rgb(32, 33, 36)" });
    expect(colors.dark).toEqual({ background: "rgb(32, 33, 36)", text: "rgb(255, 255, 255)" });
  });
}

for (const theme of ["light", "dark"] as const) {
  test(`${theme}: primary actions match the selected theme on every page`, async ({ page }) => {
    await installBackend(page, theme);
    await page.goto("/");
    const assertPrimaryActions = async () => {
      const buttons = page.locator("button.primary-button:not(:disabled), button.outline-action-button:not(:disabled)");
      const styles = await buttons.evaluateAll(elements => elements.map(element => {
        const style = getComputedStyle(element);
        return { label: element.textContent?.trim(), background: style.backgroundColor, text: style.color };
      }));
      for (const style of styles) {
        expect(style.background, style.label).toBe(theme === "light" ? "rgb(255, 255, 255)" : "rgb(57, 58, 62)");
        expect(style.text, style.label).toBe(theme === "light" ? "rgb(37, 38, 40)" : "rgb(255, 255, 255)");
      }
    };
    await expect(page.getByRole("button", { name: "添加 Provider" })).toBeVisible();
    await assertPrimaryActions(); // Add Provider
    for (const name of ["模型管理", "代理运行", "模型用量"]) {
      await page.locator(".sidebar").getByRole("button", { name, exact: true }).click();
      await assertPrimaryActions();
    }
    const terminal = page.locator(".sidebar").getByRole("button", { name: "终端管理", exact: true });
    if (await terminal.getAttribute("aria-expanded") !== "true") await terminal.click();
    await page.locator(".sidebar").getByRole("button", { name: "Codex CLI", exact: true }).click();
    await assertPrimaryActions();
  });
}
