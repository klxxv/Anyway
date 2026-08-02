/* eslint-disable @typescript-eslint/no-unused-expressions -- playwright-cli loads this file as a function expression. */
async (page) => {
  const node = page.locator(".react-flow__node").nth(2);
  const box = await node.boundingBox();
  if (!box) throw new Error("Benchmark node not found");

  await page.evaluate(() => {
    const samples = [];
    const longTasks = [];
    let previous = performance.now();
    let active = true;
    const tick = (now) => {
      samples.push(now - previous);
      previous = now;
      if (active) requestAnimationFrame(tick);
    };
    const observer = new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) longTasks.push(entry.duration);
    });
    observer.observe({ type: "longtask" });
    window.__canvasProfile = {
      samples,
      longTasks,
      stop() {
        active = false;
        observer.disconnect();
      },
    };
    requestAnimationFrame(tick);
  });

  const startX = box.x + box.width / 2;
  const startY = box.y + box.height / 2;
  await page.mouse.move(startX, startY);
  await page.mouse.down();
  const started = await page.evaluate(() => performance.now());
  await page.mouse.move(startX + 110, startY + 120, { steps: 120 });
  await page.mouse.up();
  await page.waitForTimeout(500);
  const ended = await page.evaluate(() => performance.now());

  return page.evaluate(
    ({ started, ended }) => {
      window.__canvasProfile.stop();
      const values = window.__canvasProfile.samples.slice(2).sort((a, b) => a - b);
      const quantile = (value) =>
        values[Math.min(values.length - 1, Math.floor(values.length * value))] || 0;
      return {
        durationMs: ended - started,
        frames: values.length,
        meanFrameMs:
          values.reduce((sum, value) => sum + value, 0) / Math.max(values.length, 1),
        p50FrameMs: quantile(0.5),
        p95FrameMs: quantile(0.95),
        maxFrameMs: values.length ? values[values.length - 1] : 0,
        over20ms: values.filter((value) => value > 20).length,
        over32ms: values.filter((value) => value > 32).length,
        longTasks: window.__canvasProfile.longTasks,
      };
    },
    { started, ended },
  );
}
