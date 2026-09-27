// 逐帧渲染 jpp-promo.html 为 MP4。依赖 puppeteer-core、本机 Chrome 与一个 ffmpeg 可执行文件。
// 用法：
//   FFMPEG=/path/to/ffmpeg node render.cjs --out jpp-promo.mp4 [--fps 30] [--from 0] [--to 999]
//   node render.cjs --stills 3,12,40 --stills-dir stills   # 只截静帧
const puppeteer = require("puppeteer-core");
const { spawn } = require("child_process");
const path = require("path");
const fs = require("fs");

const args = Object.fromEntries(process.argv.slice(2).reduce((a, v, i, all) => (v.startsWith("--") ? a.concat([[v.slice(2), all[i + 1]]]) : a), []));
const CHROME = process.env.CHROME || "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const page_url = "file://" + path.resolve(__dirname, args.page || "jpp-promo.html") + "?render";

(async () => {
  const browser = await puppeteer.launch({ executablePath: CHROME, headless: true, args: ["--force-color-profile=srgb", "--hide-scrollbars"] });
  const page = await browser.newPage();
  page.on("pageerror", e => console.error("page error:", e.message));
  await page.setViewport({ width: 1920, height: 1080, deviceScaleFactor: 1 });
  await page.goto(page_url, { waitUntil: "load" });
  await page.evaluate(() => document.fonts.ready);
  await page.evaluate(() => window.ready ? window.ready() : 0);
  const total = await page.evaluate(() => window.TOTAL);

  if (args.stills) {
    const dir = path.resolve(args["stills-dir"] || "stills");
    fs.mkdirSync(dir, { recursive: true });
    for (const s of args.stills.split(",").map(Number)) {
      await page.evaluate(t => window.render(t), s);
      await page.screenshot({ path: path.join(dir, `t${String(s).padStart(6, "0")}.png`) });
    }
    await browser.close();
    return;
  }

  const fps = Number(args.fps || 30);
  const from = Number(args.from || 0), to = Math.min(Number(args.to || total), total);
  const n = Math.round((to - from) * fps);
  const ff = spawn(process.env.FFMPEG || "ffmpeg", ["-y", "-f", "image2pipe", "-framerate", String(fps), "-c:v", "mjpeg", "-i", "-",
    "-vf", "scale=in_range=pc:out_range=tv,format=yuv420p", "-c:v", "libx264", "-preset", "slow", "-crf", "17",
    "-color_range", "tv", "-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709", "-movflags", "+faststart", path.resolve(args.out || "jpp-promo.mp4")], { stdio: ["pipe", "inherit", "inherit"] });
  const t0 = Date.now();
  for (let i = 0; i < n; i++) {
    await page.evaluate(t => window.render(t), from + i / fps);
    const buf = await page.screenshot({ type: "jpeg", quality: 95 });
    if (!ff.stdin.write(buf)) await new Promise(r => ff.stdin.once("drain", r));
    if (i % 150 === 0) process.stderr.write(`frame ${i}/${n}  ${((Date.now() - t0) / 1000).toFixed(0)}s\n`);
  }
  ff.stdin.end();
  await new Promise(r => ff.on("close", r));
  await browser.close();
  console.log(`done: ${n} frames, ${total.toFixed(2)}s total timeline`);
})().catch(e => { console.error(e); process.exit(1); });
