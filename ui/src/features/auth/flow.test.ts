import { describe, expect, it } from "vitest";
import { errorText, nextStep } from "./flow";

describe("nextStep", () => {
  it("waits the server interval while pending", () => {
    expect(nextStep({ status: "pending", interval: 5 })).toEqual({ kind: "wait", ms: 5000 });
    expect(nextStep({ status: "pending", interval: 0 })).toEqual({ kind: "wait", ms: 1000 });
  });
  it("finishes or stops", () => {
    expect(nextStep({ status: "done", login: "octo" })).toEqual({ kind: "done", login: "octo" });
    expect(nextStep({ status: "expired" }).kind).toBe("stop");
    expect(nextStep({ status: "denied" }).kind).toBe("stop");
  });
});

describe("errorText", () => {
  it("reads CmdError shapes", () => {
    expect(errorText({ kind: "Github", message: "network: timeout" })).toBe("network: timeout");
    expect(errorText({ kind: "Unauthorized" })).toBe("Not signed in.");
    expect(errorText("boom")).toBe("boom");
  });
});
