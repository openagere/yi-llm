import { expect, test, type Page } from "@playwright/test";
import { installBackend } from "./mock-backend";

async function expectFits(page: Page) {
  const bounds = await page.locator(".main-panel").boundingBox();
  expect(bounds!.x + bounds!.width).toBeCloseTo(await page.evaluate(() => innerWidth), 0);
  const widths = await page.locator(".main-panel").evaluate(el => ({ client: el.clientWidth, scroll: el.scrollWidth }));
  expect(widths.scroll, "The content pane must not overflow horizontally").toBeLessThanOrEqual(widths.client + 1);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(await page.evaluate(() => innerWidth));
}

for (const theme of ["light", "dark"] as const) {
  test(`${theme}: every workspace page renders in the neutral shell`, async ({ page }, testInfo) => {
    const errors: string[] = [];
    page.on("pageerror", error => errors.push(error.message));
    await installBackend(page, theme);
    await page.goto("/");
    await expect(page.locator(".connection-row")).toHaveCount(4);
    await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
    await expectFits(page);
    await page.screenshot({ path: testInfo.outputPath("providers.png") });
    for (const [name, title, ready] of [
      ["模型管理", "模型管理", ".standard-model-row"],
      ["代理运行", "代理运行", ".proxy-monitor-facts"],
      ["模型用量", "模型用量", ".analytics-summary"],
      ["终端接入", "终端接入", ".terminal-layout"],
      ["设置", "设置", ".preference-options"],
    ]) {
      await page.locator(".sidebar").getByRole("button", { name, exact: true }).click();
      await expect(page.locator("h1:visible")).toHaveText(title);
      await expect(page.locator(ready).first()).toBeVisible();
      if (name === "终端接入") await expect(page.locator(".terminal-preview")).toContainText('model_provider = "yi"');
      await expectFits(page);
      await page.screenshot({ path: testInfo.outputPath(`${name}.png`) });
    }
    expect(errors).toEqual([]);
  });

  test(`${theme}: default desktop window and narrow viewport stay usable`, async ({ page }, testInfo) => {
    await installBackend(page, theme);
    await page.goto("/");
    await expect(page.locator(".connection-row")).toHaveCount(4);
    for (const viewport of [{ width: 1120, height: 760 }, { width: 390, height: 844 }]) {
      await page.setViewportSize(viewport);
      await page.locator(".sidebar").getByRole("button", { name: "连接", exact: true }).click();
      await expectFits(page);
      await page.screenshot({ path: testInfo.outputPath(`providers-${viewport.width}.png`) });
      for (const name of ["模型管理", "代理运行", "模型用量", "终端接入", "设置"]) {
        await page.locator(".sidebar").getByRole("button", { name, exact: true }).click();
        await expect(page.locator("h1:visible")).toHaveText(name);
        await expectFits(page);
      }
    }
  });
}

test("navigation, search, theme persistence and list controls", async ({ page }) => {
  await installBackend(page, "light");
  await page.goto("/");
  await expect(page.locator(".connection-row")).toHaveCount(4);
  await page.getByRole("searchbox", { name: "搜索 Provider" }).fill("deepseek");
  await expect(page.locator(".connection-row")).toHaveCount(1);
  await page.getByRole("button", { name: "清空搜索", exact: true }).last().click();
  await page.locator(".connection-filters").getByRole("button", { name: /已停用/ }).click();
  await expect(page.locator(".connection-row")).toHaveCount(1);
  await page.locator(".connection-filters").getByRole("button", { name: /全部/ }).click();
  await page.getByRole("checkbox", { name: "DeepSeek 启用连接" }).uncheck();
  await expect(page.getByRole("checkbox", { name: "DeepSeek 启用连接" })).not.toBeChecked();
  await page.getByRole("button", { name: "OpenAI Anthropic 协议转换", exact: true }).click();
  await expect(page.getByRole("button", { name: "OpenAI Anthropic 协议转换", exact: true })).toHaveAttribute("aria-pressed", "true");
  await page.getByRole("searchbox", { name: "搜索页面" }).fill("模型");
  await expect(page.locator(".primary-nav").getByRole("button")).toHaveCount(2);
  await page.locator(".sidebar-search").getByRole("button", { name: "清空搜索" }).click();
  await page.getByRole("button", { name: "展开或收起侧栏" }).click();
  await expect(page.locator(".app-shell")).toHaveClass(/sidebar-collapsed/);
  await page.getByRole("button", { name: "展开或收起侧栏" }).click();
  await page.getByRole("button", { name: "切换浅色 / 深色外观" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  expect(await page.evaluate(() => JSON.parse(localStorage.getItem("yi-llm:settings")!).settings.theme)).toBe("dark");
  await page.reload();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await page.locator(".sidebar").getByRole("button", { name: "设置", exact: true }).click();
  await page.getByRole("button", { name: /English/ }).click();
  await expect(page.locator("h1:visible")).toHaveText("Settings");
  await expect(page.locator(".sidebar").getByRole("button", { name: "Providers", exact: true })).toBeVisible();
});

test("provider and model editors, unsaved changes, dialogs and select portals", async ({ page }, testInfo) => {
  await installBackend(page, "dark");
  await page.goto("/");
  await page.getByRole("button", { name: "添加 Provider", exact: true }).click();
  await expect(page.locator("h1:visible")).toHaveText("添加 Provider");
  await expectFits(page);
  await page.screenshot({ path: testInfo.outputPath("provider-editor.png") });
  await page.getByRole("button", { name: "添加模型", exact: true }).click();
  await page.getByRole("combobox", { name: "标准模型 1", exact: true }).click();
  const modelOptionDescription = page.locator(".standard-model-select-menu .ui-select-option-copy small");
  await expect(modelOptionDescription).toHaveText("Context 200,000");
  await expect(modelOptionDescription).not.toContainText("Effort");
  await expect(modelOptionDescription).not.toContainText("输出");
  await page.keyboard.press("Escape");
  await page.locator('input[name="name"]').fill("New connection");
  await page.locator(".sidebar").getByRole("button", { name: "模型管理", exact: true }).click();
  await expect(page.getByRole("dialog")).toBeVisible();
  await page.getByRole("button", { name: "放弃修改", exact: true }).click();
  await page.getByRole("button", { name: "添加模型", exact: true }).click();
  await expect(page.locator("h1:visible")).toHaveText("添加模型");
  await expectFits(page);
  await page.screenshot({ path: testInfo.outputPath("model-editor.png") });
  await page.getByRole("combobox", { name: "常用 Context 容量", exact: true }).click();
  await expect(page.getByRole("listbox")).toBeVisible();
  await page.getByRole("option", { name: "128K", exact: true }).click();
  await expect(page.locator(".token-capacity-input input").first()).toHaveValue("128K");
});

test("empty and unavailable backend states remain navigable", async ({ page }, testInfo) => {
  await installBackend(page, "light", true);
  await page.goto("/");
  await expect(page.locator(".connection-empty")).toBeVisible();
  await expectFits(page);
  await page.screenshot({ path: testInfo.outputPath("empty.png") });
  await page.locator(".sidebar").getByRole("button", { name: "模型管理", exact: true }).click();
  await expect(page.locator("h1:visible")).toHaveText("模型管理");
  await installBackend(page, "dark", false, true);
  await page.reload();
  await expect(page.locator(".app-toast.error")).toBeVisible();
  await expectFits(page);
  await page.locator(".sidebar").getByRole("button", { name: "设置", exact: true }).click();
  await expect(page.locator("h1:visible")).toHaveText("设置");
});


test("narrow-screen sidebar expands as a drawer without shrinking the content", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await installBackend(page, "light");
  await page.goto("/");
  await expect(page.locator(".connection-row")).toHaveCount(4);
  await expect(page.locator(".app-shell")).toHaveClass(/sidebar-collapsed/);
  await page.getByRole("button", { name: "展开或收起侧栏" }).click();
  await expect(page.getByRole("searchbox", { name: "搜索页面" })).toBeVisible();
  await expect(page.getByRole("button", { name: "收起侧栏", exact: true })).toBeVisible();
  await expectFits(page);
  await page.locator(".sidebar").getByRole("button", { name: "设置", exact: true }).click();
  await expect(page.locator("h1:visible")).toHaveText("设置");
  await expect(page.locator(".app-shell")).toHaveClass(/sidebar-collapsed/);
});

test("proxy monitor remains primary while advanced settings, tabs and runtime actions work", async ({ page }) => {
  await installBackend(page, "dark");
  await page.goto("/");
  await page.locator(".sidebar").getByRole("button", { name: "代理运行", exact: true }).click();
  await expect(page.locator(".proxy-monitor-facts")).toBeVisible();
  await expect(page.locator("#proxy-listen-settings")).not.toBeVisible();
  await page.locator(".workspace-header").getByRole("button", { name: "检查设置", exact: true }).click();
  await expect(page.locator("#proxy-listen-settings")).toBeVisible();
  await page.getByRole("button", { name: "关闭监听设置", exact: true }).click();
  await page.locator(".main-panel").evaluate(el => el.scrollTo({ top: 0 }));
  await page.getByRole("tab", { name: "协议地址", exact: true }).click();
  await expect(page.locator("#proxy-connections-panel")).toBeVisible();
  await page.getByRole("tab", { name: "运行日志", exact: true }).click();
  await expect(page.locator("#proxy-logs-panel")).toBeVisible();
  await page.getByRole("button", { name: "停止代理", exact: true }).click();
  await expect(page.getByRole("button", { name: "启动代理", exact: true })).toBeVisible();
});


for (const viewport of [{ width: 1120, height: 760 }, { width: 960, height: 640 }]) {
  test(`compact layout: default-page fit and minimum-window reflow at ${viewport.width}x${viewport.height}`, async ({ page }, testInfo) => {
    await page.setViewportSize(viewport);
    await installBackend(page, "dark", false, false, { native: true, fullPage: true });
    await page.goto("/");
    await expect(page.locator(".connection-row")).toHaveCount(6);
    for (const [name, ready] of [["连接", ".connection-row"], ["模型管理", ".standard-model-row"], ["代理运行", ".proxy-monitor-facts"], ["模型用量", ".analytics-summary"], ["终端接入", ".terminal-layout"], ["设置", ".preference-options"]]) {
      await page.locator(".sidebar").getByRole("button", { name, exact: true }).click();
      await expect(page.locator(ready).first()).toBeVisible();
      if (name === "终端接入") await expect(page.locator(".terminal-preview")).toContainText('model_provider = "yi"');
      await expectFits(page);
      const height = await page.locator(".main-panel").evaluate(el => ({ visible: el.clientHeight, full: el.scrollHeight }));
      await page.screenshot({ path: testInfo.outputPath(`${name}.png`) });
      if (viewport.width === 1120) expect.soft(height.full, `${name} should fit without scrolling`).toBeLessThanOrEqual(height.visible + 2);
    }
  });
}

test("native titlebar routes window operations and protects unsaved edits", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 1120, height: 760 });
  await installBackend(page, "light", false, false, { native: true, fullPage: false });
  await page.goto("/");
  await expect(page.getByRole("button", { name: "最小化", exact: true })).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath("native-light.png") });
  await page.getByRole("button", { name: "最大化", exact: true }).click();
  await expect(page.getByRole("button", { name: "还原窗口", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "还原窗口", exact: true }).click();
  await page.getByRole("button", { name: "最小化", exact: true }).click();
  await page.locator(".titlebar-caption").dispatchEvent("mousedown", { button: 0, detail: 1 });
  await page.locator(".titlebar-caption").dispatchEvent("dblclick", { button: 0 });
  await expect(page.getByRole("button", { name: "还原窗口", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "添加 Provider", exact: true }).click();
  await page.locator('input[name="name"]').fill("Unsaved");
  await page.getByRole("button", { name: "关闭窗口", exact: true }).click();
  await expect(page.getByRole("dialog")).toBeVisible();
  const commands = () => page.evaluate(() => (window as typeof window & { __nativeCommands: { command: string }[] }).__nativeCommands.map(item => item.command));
  expect(await commands()).not.toContain("plugin:window|close");
  await page.getByRole("button", { name: "放弃修改", exact: true }).click();
  expect(await commands()).toContain("plugin:window|close");
  expect(await commands()).toContain("plugin:window|start_dragging");
  expect(await commands()).toContain("plugin:window|minimize");
  expect((await commands()).filter(command => command === "plugin:window|toggle_maximize")).toHaveLength(3);
  expect(await commands()).toContain("plugin:window|set_theme");
});

test("resize preserves readable typography, compact navigation and native chrome", async ({ page }) => {
  await page.setViewportSize({ width: 1120, height: 760 });
  await installBackend(page, "dark", false, false, { native: true, fullPage: false });
  await page.goto("/");
  await expect(page.locator(".connection-row")).toHaveCount(4);
  const measure = () => page.evaluate(() => ({
    row: document.querySelector(".connection-row")!.getBoundingClientRect().height,
    font: getComputedStyle(document.querySelector(".connection-name > button")!).fontSize,
    sidebar: document.querySelector(".sidebar")!.getBoundingClientRect().width,
    chrome: document.querySelector(".workspace-toolbar")!.getBoundingClientRect().height,
  }));
  const before = await measure();
  for (const viewport of [{ width: 960, height: 640 }, { width: 1440, height: 960 }]) {
    await page.setViewportSize(viewport);
    const after = await measure();
    expect(after.font).toBe(before.font);
    expect(after.row).toBe(before.row);
    expect(after.chrome).toBe(36);
    await expectFits(page);
  }
  expect(before.sidebar / 1120).toBeLessThan(.19);
});


for (const theme of ["light", "dark"] as const) {
  test(`${theme}: client protocol chips share a baseline, icon scale and state layout`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width: 1120, height: 760 });
    await installBackend(page, theme, false, false, { native: true, fullPage: true });
    await page.goto("/");
    await expect(page.locator(".connection-row")).toHaveCount(6);
    await expect(page.locator(".connection-protocols .provider-brand-icon")).toHaveCount(18);
    await expect(page.locator(".connection-name .provider-brand-icon")).toHaveCount(6);
    await expect(page.locator(".connection-protocol.native")).toHaveCount(6);
    await expect(page.locator(".connection-protocol.native .lucide-lock-keyhole")).toHaveCount(6);
    const checkAlignment = async () => {
      const metadata = await page.locator(".connection-name").evaluateAll(names => names.map(name => {
        const alias = name.querySelector(".connection-alias")!.getBoundingClientRect();
        const models = name.querySelector(".connection-meta")!.getBoundingClientRect();
        return { followsAlias: models.x >= alias.right, delta: Math.abs(models.y + models.height / 2 - alias.y - alias.height / 2) };
      }));
      for (const item of metadata) { expect(item.followsAlias).toBe(true); expect(item.delta).toBeLessThanOrEqual(.5); }
      const measurements = await page.locator(".connection-protocol").evaluateAll(chips => chips.map(chip => {
        const icon = chip.querySelector(".connection-protocol-mark > .provider-brand-icon")!.getBoundingClientRect();
        const label = chip.querySelector(".connection-protocol-copy") as HTMLElement;
        const text = label.getBoundingClientRect();
        const state = chip.querySelector(".connection-protocol-state")!.getBoundingClientRect();
        return { height: chip.getBoundingClientRect().height, width: icon.width, iconHeight: icon.height, delta: icon.y + icon.height / 2 - text.y - text.height / 2, stateDelta: Math.abs(state.y + state.height / 2 - text.y - text.height / 2), clipped: label.scrollWidth > label.clientWidth + 1 };
      }));
      for (const item of measurements) {
        expect(item.width).toBe(14);
        expect(item.iconHeight).toBe(14);
        expect(Math.abs(item.delta - 1)).toBeLessThanOrEqual(.5);
        expect(item.stateDelta).toBeLessThanOrEqual(.5);
        expect(item.clipped).toBe(false);
      }
      expect(new Set(measurements.map(item => item.height)).size).toBe(1);
    };
    await checkAlignment();
    await page.screenshot({ path: testInfo.outputPath("client-protocols.png") });
    await page.locator(".connection-row").first().screenshot({ path: testInfo.outputPath("protocol-row.png") });
    const conversion = page.getByRole("button", { name: "OpenAI Anthropic 协议转换", exact: true });
    await expect(conversion).toHaveAttribute("aria-pressed", "false");
    await conversion.focus();
    await page.keyboard.press("Space");
    await expect(conversion).toHaveAttribute("aria-pressed", "true");
    await expect(conversion.locator(".connection-protocol-state .lucide-check")).toBeVisible();
    await checkAlignment();
    await page.setViewportSize({ width: 390, height: 844 });
    await checkAlignment();
    await expectFits(page);
  });
}


for (const theme of ["light", "dark"] as const) {
  test(`${theme}: Yi branding has no titlebar back arrow or page text`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width: 1120, height: 760 });
    await installBackend(page, theme, false, false, { native: true, fullPage: false });
    await page.goto("/");
    await expect(page.locator(".connection-row")).toHaveCount(4);
    const toolbar = page.locator(".workspace-toolbar");
    await expect(toolbar.getByRole("button", { name: "返回上一页", exact: true })).toHaveCount(0);
    await expect(toolbar.locator(".lucide-arrow-left")).toHaveCount(0);
    await expect(toolbar.locator(".toolbar-page-name")).toHaveCount(0);
    await expect(toolbar).not.toContainText("连接");
    await expect(toolbar).not.toContainText("LLM Man");
    await expect(page.locator(".sidebar")).not.toContainText("LLM Man");
    await expect(page).toHaveTitle("工作区");
    const marks = page.locator(".app-logo > svg");
    await expect(marks).toHaveCount(2);
    const sourcePath = await page.evaluate(async () => {
      const source = await fetch("/src/assets/providers/yi.svg").then(response => response.text());
      return new DOMParser().parseFromString(source, "image/svg+xml").querySelector("path")!.getAttribute("d");
    });
    for (const mark of await marks.all()) {
      await expect(mark.locator("path")).toHaveAttribute("d", sourcePath!);
      const colors = await mark.evaluate(svg => ({ fill: getComputedStyle(svg.querySelector("path")!).fill, color: getComputedStyle(svg).color, dot: getComputedStyle(svg.querySelector("ellipse")!).fill }));
      expect(colors.fill).toBe(colors.color);
      expect(colors.dot).toBe("rgb(0, 255, 37)");
    }
    const bounds = await toolbar.locator(".titlebar-logo-slot").boundingBox();
    expect(bounds!.x).toBe(10);
    expect(bounds!.width).toBe(28);
    expect((await toolbar.locator(".titlebar-logo > svg").boundingBox())!.width).toBe(18);
    expect((await page.locator(".brand .app-logo > svg").boundingBox())!.width).toBe(24);
    await page.screenshot({ path: testInfo.outputPath("yi-branding.png") });
    await page.locator(".sidebar").getByRole("button", { name: "模型管理", exact: true }).click();
    await page.getByRole("button", { name: "添加模型", exact: true }).click();
    await expect(page.locator(".workspace-header").getByRole("button", { name: "返回模型管理", exact: true })).toBeVisible();
  });
}
