import { describe, expect, it } from "vitest";
import { minutesUntilShopUnlock } from "./shopUnlock";

describe("minutesUntilShopUnlock", () => {
  it("counts remaining minutes until 60, then stays at 0", () => {
    expect(minutesUntilShopUnlock(10)).toBe(50);
    expect(minutesUntilShopUnlock(60)).toBe(0);
    expect(minutesUntilShopUnlock(90)).toBe(0);
  });
});
