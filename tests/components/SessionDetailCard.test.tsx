import { fireEvent, render, screen } from "@testing-library/react";
import type { ComponentProps } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { TFunction } from "i18next";
import type { SessionMessage, SessionMeta } from "@/types";
import { SessionDetailCard } from "@/components/sessions/SessionDetailCard";
import { TooltipProvider } from "@/components/ui/tooltip";

vi.mock("@/lib/platform", () => ({
  isMac: () => true,
}));

vi.mock("@/components/sessions/SessionToc", () => ({
  SessionTocSidebar: () => null,
  SessionTocDialog: () => null,
}));

const t = ((key: string, options?: { defaultValue?: string }) =>
  options?.defaultValue ?? key) as TFunction;

const session: SessionMeta = {
  providerId: "codex",
  sessionId: "session-1",
  title: "Alpha Session",
  summary: "Alpha summary",
  projectDir: "/workspace/demo",
  createdAt: 1,
  lastActiveAt: 2,
  sourcePath: "/workspace/demo/session.jsonl",
  resumeCommand: "codex resume session-1",
};

const messages: SessionMessage[] = [
  { role: "user", content: "hello from user", ts: 2 },
  { role: "assistant", content: "hello from assistant", ts: 3 },
];

function renderCard(
  overrides: Partial<ComponentProps<typeof SessionDetailCard>> = {},
) {
  const props: ComponentProps<typeof SessionDetailCard> = {
    t,
    selectedSession: session,
    messages,
    isLoadingMessages: false,
    isDeleting: false,
    onCopy: vi.fn(),
    onResume: vi.fn(),
    onDelete: vi.fn(),
    ...overrides,
  };

  render(
    <TooltipProvider>
      <SessionDetailCard {...props} />
    </TooltipProvider>,
  );

  return props;
}

describe("SessionDetailCard", () => {
  beforeEach(() => {
    Element.prototype.scrollIntoView = vi.fn();
  });

  it("renders the empty selection state", () => {
    renderCard({
      selectedSession: null,
      messages: [],
    });

    expect(screen.getByText("sessionManager.selectSession")).toBeInTheDocument();
  });

  it("renders session metadata and messages", () => {
    renderCard();

    expect(
      screen.getByRole("heading", { name: "Alpha Session" }),
    ).toBeInTheDocument();
    expect(screen.getByText("demo")).toBeInTheDocument();
    expect(screen.getByText("hello from user")).toBeInTheDocument();
    expect(screen.getByText("hello from assistant")).toBeInTheDocument();
  });

  it("forwards copy actions for project path, resume command and message", () => {
    const onCopy = vi.fn();
    renderCard({ onCopy });

    fireEvent.click(screen.getByRole("button", { name: "demo" }));
    fireEvent.click(screen.getAllByRole("button")[3]);
    fireEvent.click(screen.getAllByRole("button")[4]);

    expect(onCopy).toHaveBeenNthCalledWith(
      1,
      "/workspace/demo",
      "sessionManager.projectDirCopied",
    );
    expect(onCopy).toHaveBeenNthCalledWith(
      2,
      "codex resume session-1",
      "sessionManager.resumeCommandCopied",
    );
    expect(onCopy).toHaveBeenNthCalledWith(
      3,
      "hello from user",
      "已复制消息内容",
    );
  });

  it("forwards resume and delete actions", () => {
    const onResume = vi.fn();
    const onDelete = vi.fn();
    renderCard({ onResume, onDelete });

    fireEvent.click(screen.getByRole("button", { name: /恢复会话/i }));
    fireEvent.click(screen.getByRole("button", { name: /删除会话/i }));

    expect(onResume).toHaveBeenCalledTimes(1);
    expect(onDelete).toHaveBeenCalledTimes(1);
  });

  it("disables delete when the session has no source path", () => {
    renderCard({
      selectedSession: {
        ...session,
        sourcePath: undefined,
      },
    });

    expect(screen.getByRole("button", { name: /删除会话/i })).toBeDisabled();
  });
});
