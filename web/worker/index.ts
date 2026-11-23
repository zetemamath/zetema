// The site's only server code: it hands out the lab's latest state and lets the lab's server replace it.
//
// GET  /data/lab.json  the state, cached for 30 seconds
// POST /data/push      the lab's server uploads a new state; it must present the shared key

interface Bucket {
  get(key: string): Promise<{ body: ReadableStream } | null>;
  put(key: string, value: string, options?: { httpMetadata?: { contentType?: string } }): Promise<unknown>;
}

interface Env {
  DATA: Bucket;
  PUSH_KEY: string;
}

const LIMIT = 512 * 1024;

function same(a: string, b: string): boolean {
  const [x, y] = [new TextEncoder().encode(a), new TextEncoder().encode(b)];
  let diff = x.length ^ y.length;
  for (let i = 0; i < Math.max(x.length, y.length); i++) diff |= (x[i] ?? 0) ^ (y[i] ?? 0);
  return diff === 0;
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const { pathname } = new URL(request.url);
    if (pathname === "/data/lab.json" && (request.method === "GET" || request.method === "HEAD")) {
      const state = await env.DATA.get("lab.json");
      if (!state) return new Response("{}", { status: 404, headers: { "content-type": "application/json" } });
      return new Response(state.body, {
        headers: { "content-type": "application/json", "cache-control": "public, max-age=30", "access-control-allow-origin": "*" },
      });
    }
    if (pathname === "/data/push" && request.method === "POST") {
      if (!env.PUSH_KEY || !same(request.headers.get("authorization") ?? "", `Bearer ${env.PUSH_KEY}`)) {
        return new Response("unauthorized", { status: 401 });
      }
      const text = await request.text();
      if (text.length > LIMIT) return new Response("too large", { status: 413 });
      try {
        JSON.parse(text);
      } catch {
        return new Response("not JSON", { status: 400 });
      }
      await env.DATA.put("lab.json", text, { httpMetadata: { contentType: "application/json" } });
      return new Response(null, { status: 204 });
    }
    return new Response("not found", { status: 404 });
  },
};
