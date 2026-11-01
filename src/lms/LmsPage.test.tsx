/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { createMemoryHistory, createRouter, RouterProvider } from "@tanstack/react-router";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { routeTree } from "../routeTree.gen";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: class MockChannel {
    onmessage = null;
  },
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

type Listener = (event: { payload: unknown }) => void;
const listeners = new Map<string, Listener>();

function snapshot(title: string) {
  return {
    session: "ready",
    message: "",
    profile: { uid: 1, name: "测试", userNo: "2025150041" },
    todos: [
      {
        course_code: "2026",
        course_id: 9,
        course_name: "数据库",
        course_type: 1,
        end_time: "2099-01-01T00:00:00Z",
        id: 1,
        is_locked: false,
        is_student: true,
        prerequisites: [],
        title,
        type: "homework",
      },
    ],
    notifications: [],
    unreadCount: 0,
    fetchedAt: 1,
  };
}

function mount() {
  const router = createRouter({
    routeTree,
    history: createMemoryHistory({ initialEntries: ["/study/lms"] }),
  });
  render(<RouterProvider router={router} />);
  return router;
}

beforeEach(() => {
  vi.clearAllMocks();
  listeners.clear();
  vi.mocked(listen).mockImplementation((async (event: string, handler: Listener) => {
    listeners.set(event, handler);
    return () => listeners.delete(event);
  }) as unknown as typeof listen);
  vi.mocked(invoke).mockImplementation(async (command: string) => {
    if (command === "get_auth_state") {
      return { Authenticated: { studentId: "2025150041", name: "测试" } };
    }
    if (command === "get_lms_snapshot" || command === "refresh_lms") {
      return snapshot("第一次作业");
    }
    if (command === "get_onboarding_state") {
      return { steps: [], needsSetup: false, currentStep: null };
    }
    return null;
  });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

it("renders cached todos and updates silently on lms-state-changed", async () => {
  mount();
  expect(await screen.findByText("第一次作业")).toBeInTheDocument();
  await waitFor(() => expect(listeners.has("lms-state-changed")).toBe(true));

  await act(async () => {
    listeners.get("lms-state-changed")?.({ payload: snapshot("第二次作业") });
  });

  await waitFor(() => expect(screen.getByText("第二次作业")).toBeInTheDocument());
  expect(screen.queryByText("第一次作业")).not.toBeInTheDocument();
});
