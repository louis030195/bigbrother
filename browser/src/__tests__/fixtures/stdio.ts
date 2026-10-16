// screenpipe — AI that knows everything you've seen, said, and heard
// https://screenpipe.com
import { chromium } from "playwright";
import { startServer } from "../../server.js";
async function main() {
  const browser = await chromium.launch({ headless: true });
  try {
    const context = await browser.newContext();
    const page = await context.newPage();
    await page.setContent("<button>Save</button>");
    await startServer({
      browser,
      context,
      page,
      profile: null,
      cookieStats: { total: 0, injected: 0, skipped: 0, errors: [] },
      cleanup: async () => {
        await context.close();
      },
    });
  } finally {
    await browser.close();
  }
}
main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
