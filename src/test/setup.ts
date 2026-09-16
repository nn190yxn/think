import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach, beforeEach } from "vitest";

beforeEach(() => {
  window.localStorage.clear();
  document.documentElement.dataset["theme"] = "kiln";
});

afterEach(() => {
  cleanup();
});
