import { describe, expect, it } from "vitest";
import { splitWishes } from "./shopSplit";

describe("splitWishes", () => {
  it("splits coin and energy", () => {
    const { coin, energy } = splitWishes([
      { id: "a", name: "茶", kind: "coin", price: 32, durationMinutes: null },
      { id: "b", name: "B站", kind: "xp", price: 20, durationMinutes: 45 },
    ]);
    expect(coin.map((w) => w.id)).toEqual(["a"]);
    expect(energy.map((w) => w.id)).toEqual(["b"]);
  });
});
