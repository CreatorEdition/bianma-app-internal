import { useMemo, useRef, useState } from "react";
import type { TFunction } from "i18next";
import {
  Clock,
  Copy,
  FolderOpen,
  MessageSquare,
  Play,
  RefreshCw,
  Trash2,
} from "lucide-react";
import type { SessionMessage, SessionMeta } from "@/types";
import { isMac } from "@/lib/platform";
import { cn } from "@/lib/utils";
import { ProviderIcon } from "@/components/ProviderIcon";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { SessionMessageItem } from "./SessionMessageItem";
import { SessionTocDialog, SessionTocSidebar } from "./SessionToc";
import {
  formatSessionTitle,
  formatTimestamp,
  getBaseName,
  getProviderIconName,
  getProviderLabel,
} from "./utils";

interface SessionDetailCardProps {
  t: TFunction;
  selectedSession: SessionMeta | null;
  messages: SessionMessage[];
  isLoadingMessages: boolean;
  isDeleting: boolean;
  onCopy: (text: string, successMessage: string) => void | Promise<void>;
  onResume: () => void | Promise<void>;
  onDelete: () => void;
  className?: string;
}

export function SessionDetailCard({
  t,
  selectedSession,
  messages,
  isLoadingMessages,
  isDeleting,
  onCopy,
  onResume,
  onDelete,
  className,
}: SessionDetailCardProps) {
  const messageRefs = useRef<Map<number, HTMLDivElement>>(new Map());
  const [activeMessageIndex, setActiveMessageIndex] = useState<number | null>(
    null,
  );
  const [tocDialogOpen, setTocDialogOpen] = useState(false);

  const userMessagesToc = useMemo(() => {
    return messages
      .map((msg, index) => ({ msg, index }))
      .filter(({ msg }) => msg.role.toLowerCase() === "user")
      .map(({ msg, index }) => ({
        index,
        preview:
          msg.content.slice(0, 50) + (msg.content.length > 50 ? "..." : ""),
        ts: msg.ts,
      }));
  }, [messages]);

  const scrollToMessage = (index: number) => {
    const element = messageRefs.current.get(index);
    if (!element) return;

    element.scrollIntoView({ behavior: "smooth", block: "center" });
    setActiveMessageIndex(index);
    setTocDialogOpen(false);
    setTimeout(() => setActiveMessageIndex(null), 2000);
  };

  return (
    <Card className={cn("flex flex-col overflow-hidden min-h-0", className)}>
      {!selectedSession ? (
        <div className="flex-1 flex flex-col items-center justify-center text-muted-foreground p-8">
          <MessageSquare className="size-12 mb-3 opacity-30" />
          <p className="text-sm">{t("sessionManager.selectSession")}</p>
        </div>
      ) : (
        <>
          <CardHeader className="py-3 px-4 border-b shrink-0">
            <div className="flex items-start justify-between gap-4">
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2 mb-1">
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <span className="shrink-0">
                        <ProviderIcon
                          icon={getProviderIconName(selectedSession.providerId)}
                          name={selectedSession.providerId}
                          size={20}
                        />
                      </span>
                    </TooltipTrigger>
                    <TooltipContent>
                      {getProviderLabel(selectedSession.providerId, t)}
                    </TooltipContent>
                  </Tooltip>
                  <h2 className="text-base font-semibold truncate">
                    {formatSessionTitle(selectedSession)}
                  </h2>
                </div>

                <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-muted-foreground">
                  <div className="flex items-center gap-1">
                    <Clock className="size-3" />
                    <span>
                      {formatTimestamp(
                        selectedSession.lastActiveAt ??
                          selectedSession.createdAt,
                      )}
                    </span>
                  </div>
                  {selectedSession.projectDir && (
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <button
                          type="button"
                          onClick={() =>
                            void onCopy(
                              selectedSession.projectDir!,
                              t("sessionManager.projectDirCopied"),
                            )
                          }
                          className="flex items-center gap-1 hover:text-foreground transition-colors"
                        >
                          <FolderOpen className="size-3" />
                          <span className="truncate max-w-[200px]">
                            {getBaseName(selectedSession.projectDir)}
                          </span>
                        </button>
                      </TooltipTrigger>
                      <TooltipContent side="bottom" className="max-w-xs">
                        <p className="font-mono text-xs break-all">
                          {selectedSession.projectDir}
                        </p>
                        <p className="text-muted-foreground mt-1">
                          {t("sessionManager.clickToCopyPath")}
                        </p>
                      </TooltipContent>
                    </Tooltip>
                  )}
                </div>
              </div>

              <div className="flex items-center gap-2 shrink-0">
                {isMac() && (
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Button
                        size="sm"
                        className="gap-1.5"
                        onClick={() => void onResume()}
                        disabled={!selectedSession.resumeCommand}
                      >
                        <Play className="size-3.5" />
                        <span className="hidden sm:inline">
                          {t("sessionManager.resume", {
                            defaultValue: "恢复会话",
                          })}
                        </span>
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent>
                      {selectedSession.resumeCommand
                        ? t("sessionManager.resumeTooltip", {
                            defaultValue: "在终端中恢复此会话",
                          })
                        : t("sessionManager.noResumeCommand", {
                            defaultValue: "此会话无法恢复",
                          })}
                    </TooltipContent>
                  </Tooltip>
                )}
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      size="sm"
                      variant="destructive"
                      className="gap-1.5"
                      onClick={onDelete}
                      disabled={!selectedSession.sourcePath || isDeleting}
                    >
                      <Trash2 className="size-3.5" />
                      <span className="hidden sm:inline">
                        {isDeleting
                          ? t("sessionManager.deleting", {
                              defaultValue: "删除中...",
                            })
                          : t("sessionManager.delete", {
                              defaultValue: "删除会话",
                            })}
                      </span>
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>
                    {t("sessionManager.deleteTooltip", {
                      defaultValue: "永久删除此本地会话记录",
                    })}
                  </TooltipContent>
                </Tooltip>
              </div>
            </div>

            {selectedSession.resumeCommand && (
              <div className="mt-3 flex items-center gap-2">
                <div className="flex-1 rounded-md bg-muted/60 px-3 py-1.5 font-mono text-xs text-muted-foreground truncate">
                  {selectedSession.resumeCommand}
                </div>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      variant="ghost"
                      size="icon"
                      className="size-7 shrink-0"
                      onClick={() =>
                        void onCopy(
                          selectedSession.resumeCommand!,
                          t("sessionManager.resumeCommandCopied"),
                        )
                      }
                    >
                      <Copy className="size-3.5" />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>
                    {t("sessionManager.copyCommand", {
                      defaultValue: "复制命令",
                    })}
                  </TooltipContent>
                </Tooltip>
              </div>
            )}
          </CardHeader>

          <CardContent className="flex-1 min-h-0 p-0">
            <div className="flex h-full min-w-0">
              <ScrollArea className="flex-1 min-w-0">
                <div className="p-4 min-w-0">
                  <div className="flex items-center gap-2 mb-3">
                    <MessageSquare className="size-4 text-muted-foreground" />
                    <span className="text-sm font-medium">
                      {t("sessionManager.conversationHistory", {
                        defaultValue: "对话记录",
                      })}
                    </span>
                    <Badge variant="secondary" className="text-xs">
                      {messages.length}
                    </Badge>
                  </div>

                  {isLoadingMessages ? (
                    <div className="flex items-center justify-center py-12">
                      <RefreshCw className="size-5 animate-spin text-muted-foreground" />
                    </div>
                  ) : messages.length === 0 ? (
                    <div className="flex flex-col items-center justify-center py-12 text-center">
                      <MessageSquare className="size-8 text-muted-foreground/50 mb-2" />
                      <p className="text-sm text-muted-foreground">
                        {t("sessionManager.emptySession")}
                      </p>
                    </div>
                  ) : (
                    <div className="space-y-3">
                      {messages.map((message, index) => (
                        <SessionMessageItem
                          key={`${message.role}-${index}`}
                          message={message}
                          index={index}
                          isActive={activeMessageIndex === index}
                          setRef={(el) => {
                            if (el) {
                              messageRefs.current.set(index, el);
                              return;
                            }
                            messageRefs.current.delete(index);
                          }}
                          onCopy={(content) =>
                            onCopy(
                              content,
                              t("sessionManager.messageCopied", {
                                defaultValue: "已复制消息内容",
                              }),
                            )
                          }
                        />
                      ))}
                    </div>
                  )}
                </div>
              </ScrollArea>

              <SessionTocSidebar
                items={userMessagesToc}
                onItemClick={scrollToMessage}
              />
            </div>

            <SessionTocDialog
              items={userMessagesToc}
              onItemClick={scrollToMessage}
              open={tocDialogOpen}
              onOpenChange={setTocDialogOpen}
            />
          </CardContent>
        </>
      )}
    </Card>
  );
}
