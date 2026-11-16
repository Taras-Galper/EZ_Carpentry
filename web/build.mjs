import * as esbuild from "esbuild";
import fs from "node:fs";
const r = await esbuild.build({
  entryPoints: ["src/main.js"], bundle: true, minify: true, format: "iife", write: false,
  loader: { ".wasm": "binary" }, target: "es2020",
});
const js = r.outputFiles[0].text.replace(/<\/script/gi, "<\\/script");
const html = fs.readFileSync("src/index.html", "utf8").replace("<script>/*BUNDLE*/</script>", () => `<script>${js}</script>`);
fs.mkdirSync("dist", { recursive: true });
fs.writeFileSync("dist/index.html", html);
console.log("dist/index.html", (html.length / 1024).toFixed(0), "KB");
