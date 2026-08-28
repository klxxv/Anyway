async (page) => {
  await page.getByRole("button", { name: "新增", exact: true }).click();
  const radial = page.locator(".zen-pie-menu");
  await radial.waitFor({ state: "visible" });
  const radialItems = radial.locator(".zen-pie-item");
  const radialItemCount = await radialItems.count();
  await radialItems.nth(3).hover();
  const hoveredRadialColor = await radialItems.nth(3).evaluate(
    (element) => getComputedStyle(element).color,
  );
  await radial.locator(".zen-pie-center").click();

  const node = page.locator(".vue-flow__node").nth(2);
  const box = await node.boundingBox();
  if (!box) throw new Error("Smoke-test node not found");
  const labels = page.locator(".vue-flow__edge-label");
  const beforeLabels = await labels.count();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 + 48, box.y + box.height / 2 + 36, {
    steps: 10,
  });
  const duringLabels = await labels.count();
  await page.mouse.up();
  await page.waitForTimeout(250);
  const afterLabels = await labels.count();

  return {
    radialItemCount,
    hoveredRadialColor,
    beforeLabels,
    duringLabels,
    afterLabels,
  };
}
