import { test } from "@playwright/test";
import { installBackend } from "./mock-backend";

for (const theme of ["light", "dark"] as const) {
  test(`visual check ${theme}`, async ({ page }, testInfo) => {
    await installBackend(page, theme);
    await page.goto("/");
    await page.getByRole("button", { name: "导出配置", exact: true }).click();
    await page.screenshot({ path: testInfo.outputPath(`export-${theme}.png`) });
    await page.getByRole("button", { name: "取消", exact: true }).click();
    await page.getByRole("button", { name: "导入配置", exact: true }).click();
    await page.screenshot({ path: testInfo.outputPath(`import-${theme}.png`) });
  });
}
