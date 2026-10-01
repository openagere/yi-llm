import { expect, test } from "@playwright/test";
import { installBackend } from "./mock-backend";

test("provider editor styles the capability summary on first open", async ({ page }) => {
  await installBackend(page, "dark");
  await page.goto("/");
  // Open the editor straight from the provider list, without visiting 模型管理 first:
  // the models CSS used to load only with the model pages, so the shared summary
  // rendered unstyled (block-level chips, icons stacked above their labels).
  await page.getByRole("button", { name: "OpenAI" }).first().click();
  const summary = page.locator(".model-capability-summary").first();
  await expect(summary).toBeVisible();
  await expect
    .poll(async () => summary.evaluate((el) => getComputedStyle(el).display))
    .toBe("flex");
  const chipWidths = await summary
    .locator("> span")
    .evaluateAll((els) => els.map((el) => el.getBoundingClientRect().width));
  expect(chipWidths.length).toBeGreaterThan(0);
  for (const width of chipWidths) {
    expect(width, "capability chips must stay compact on the first open").toBeLessThan(200);
  }
});


test("non-standard switch sits after the standard model and persists per mapping", async ({ page }, testInfo) => {
  await installBackend(page, "dark");
  await page.goto("/");
  await page.getByRole("button", { name: "OpenAI" }).first().click();
  const row = page.locator(".connection-model-row").first();
  const standard = row.locator(".provider-standard-model-select");
  const toggle = row.getByRole("checkbox", { name: "模型 1 非标准兼容" });
  const remove = row.getByRole("button", { name: "移除模型 1" });
  await expect(toggle).toBeVisible();
  await expect(page.locator(".provider-model-binding").first()).not.toContainText("gpt(oa)");
  const [standardBox, toggleBox, removeBox] = await Promise.all([
    standard.boundingBox(), toggle.boundingBox(), remove.boundingBox(),
  ]);
  expect(standardBox!.x + standardBox!.width).toBeLessThan(toggleBox!.x + 1);
  expect(toggleBox!.x + toggleBox!.width).toBeLessThan(removeBox!.x + 1);
  await expect(page.locator(".provider-model-compatibility")).toHaveCount(0);
  await toggle.check();
  await page.screenshot({ path: testInfo.outputPath("provider-non-standard.png") });
  await page.getByRole("button", { name: "保存修改" }).first().click();
  await page.getByRole("button", { name: "OpenAI" }).first().click();
  await expect(page.getByRole("checkbox", { name: "模型 1 非标准兼容" })).toBeChecked();
  await page.setViewportSize({ width: 1120, height: 760 });
  await expect(page.getByRole("checkbox", { name: "模型 1 非标准兼容" })).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath("provider-non-standard-1120.png") });
});
