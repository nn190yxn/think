import { describe, expect, it } from "vitest";
import { DEFAULT_REALM, REALM_ORDER, isRealmKey, realmOf, stepRealm } from "./realms";

describe("境界", () => {
  it("五个境界按观会炼藏我排列", () => {
    expect(REALM_ORDER).toEqual(["observe", "council", "refine", "vault", "self"]);
    expect(DEFAULT_REALM).toBe("observe");
    expect(REALM_ORDER.map((key) => realmOf(key).sigil).join("")).toBe("观会炼藏我");
  });

  it("校验境界键", () => {
    expect(isRealmKey("council")).toBe(true);
    expect(isRealmKey("forum")).toBe(false);
  });

  it("前进与后退都在两端回绕", () => {
    expect(stepRealm("observe", -1)).toBe("self");
    expect(stepRealm("self", 1)).toBe("observe");
    expect(stepRealm("observe", 1)).toBe("council");
    expect(stepRealm("self", 5)).toBe("self");
  });
});
