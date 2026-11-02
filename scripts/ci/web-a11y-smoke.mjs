import { chromium } from "playwright";
import AxeBuilder from "@axe-core/playwright";

async function run() {
  const urls = process.argv.slice(2);
  if (urls.length === 0) {
    console.error("No URLs provided to web-a11y-smoke.");
    process.exit(1);
  }

  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext();
  const page = await context.newPage();

  let hasViolations = false;

  for (const url of urls) {
    await page.goto(url, { waitUntil: "networkidle" });
    const results = await new AxeBuilder({ page }).analyze();

    if (results.violations.length > 0) {
      hasViolations = true;
      console.error(`\n[A11Y] ${url} has ${results.violations.length} violation(s):`);
      for (const violation of results.violations) {
        console.error(`- ${violation.id}: ${violation.help}`);
        for (const node of violation.nodes) {
          console.error(`  target: ${node.target.join(" > ")}`);
          if (node.failureSummary) {
            console.error(`  ${node.failureSummary.replace(/\n/g, "\n  ")}`);
          }
        }
      }
    } else {
      console.log(`[A11Y] ${url} passed`);
    }
  }

  await page.close();
  await context.close();
  await browser.close();

  if (hasViolations) {
    process.exit(1);
  }
}

run().catch((err) => {
  console.error(err);
  process.exit(1);
});
