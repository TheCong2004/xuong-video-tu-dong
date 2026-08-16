import {
  CAPCUT_BE_BASE_URL,
  pingBackend,
} from "../../app/src/pages/PageCapCutAutomation/api/capcutMateClient";

describe("packaged backend discovery", () => {
  const storage = new Map<string, string>();

  beforeEach(() => {
    storage.clear();
    Object.defineProperty(globalThis, "localStorage", {
      configurable: true,
      value: {
        getItem: jest.fn((key: string) => storage.get(key) ?? null),
        setItem: jest.fn((key: string, value: string) => storage.set(key, value)),
      },
    });
  });

  afterEach(() => {
    jest.restoreAllMocks();
    Reflect.deleteProperty(globalThis, "fetch");
    Reflect.deleteProperty(globalThis, "localStorage");
  });

  it("recovers from a stale development URL when the packaged backend is healthy", async () => {
    storage.set("capcut-mate-base-url", "http://127.0.0.1:39999");
    const fetchMock = jest.fn(async (input: string | URL | Request) => {
      const url = String(input);
      return { ok: url === `${CAPCUT_BE_BASE_URL}/health` } as Response;
    });
    Object.defineProperty(globalThis, "fetch", {
      configurable: true,
      value: fetchMock,
    });

    await expect(pingBackend({ retries: 0 })).resolves.toBe(true);
    expect(fetchMock).toHaveBeenCalledWith(
      "http://127.0.0.1:39999/health",
      expect.any(Object),
    );
    expect(fetchMock).toHaveBeenCalledWith(
      `${CAPCUT_BE_BASE_URL}/health`,
      expect.any(Object),
    );
    expect(storage.get("capcut-mate-base-url")).toBe(CAPCUT_BE_BASE_URL);
  });
});
