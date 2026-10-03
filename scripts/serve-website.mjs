import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../website/", import.meta.url));
const types = {
  ".html": "text/html; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".woff2": "font/woff2",
  ".txt": "text/plain; charset=utf-8",
};
const server = createServer(async (request, response) => {
  try {
    let pathname = decodeURIComponent(
      new URL(request.url, "http://localhost").pathname,
    );
    if (pathname === "/") {
      response.writeHead(302, { Location: "/babelhack/" });
      response.end();
      return;
    }
    if (!pathname.startsWith("/babelhack/")) {
      response.writeHead(404);
      response.end();
      return;
    }
    pathname = pathname.slice("/babelhack/".length);
    const file = path.resolve(root, pathname || "index.html");
    if (!file.startsWith(root)) {
      response.writeHead(403);
      response.end();
      return;
    }
    const body = await readFile(file);
    response.writeHead(200, {
      "Content-Type": types[path.extname(file)] || "application/octet-stream",
      "Cache-Control": "no-store",
      "X-Content-Type-Options": "nosniff",
    });
    response.end(body);
  } catch {
    response.writeHead(404);
    response.end("Not found");
  }
});
server.listen(1431, "127.0.0.1", () =>
  console.log("Babel Hack website: http://127.0.0.1:1431/babelhack/"),
);
