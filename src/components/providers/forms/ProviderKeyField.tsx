import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  isProviderKeyValid,
  normalizeProviderKeyInput,
} from "./providerKeyUtils";

interface ProviderKeyFieldProps {
  inputId: string;
  label: string;
  value: string;
  placeholder: string;
  existingKeys: string[];
  isEditMode: boolean;
  isLocked?: boolean;
  isLoading?: boolean;
  duplicateMessage: string;
  invalidMessage: string;
  hintMessage: string;
  lockedHintMessage?: string;
  loadingMessage?: string;
  onChange: (value: string) => void;
}

/**
 * OpenCode/OpenClaw 共享的供应商标识输入框，集中处理归一化与状态提示。
 */
export function ProviderKeyField({
  inputId,
  label,
  value,
  placeholder,
  existingKeys,
  isEditMode,
  isLocked = false,
  isLoading = false,
  duplicateMessage,
  invalidMessage,
  hintMessage,
  lockedHintMessage,
  loadingMessage,
  onChange,
}: ProviderKeyFieldProps) {
  const trimmedValue = value.trim();
  const isFieldLocked = isEditMode && (isLocked || isLoading);
  const isDuplicate = existingKeys.includes(value) && !isFieldLocked;
  const isInvalid = trimmedValue !== "" && !isProviderKeyValid(value);
  const showHint = !isDuplicate && (trimmedValue === "" || !isInvalid);

  return (
    <div className="space-y-2">
      <Label htmlFor={inputId}>
        {label}
        <span className="text-destructive ml-1">*</span>
      </Label>
      <Input
        id={inputId}
        value={value}
        onChange={(e) => onChange(normalizeProviderKeyInput(e.target.value))}
        placeholder={placeholder}
        disabled={isFieldLocked}
        className={isDuplicate || isInvalid ? "border-destructive" : ""}
      />
      {isDuplicate && (
        <p className="text-xs text-destructive">{duplicateMessage}</p>
      )}
      {isInvalid && (
        <p className="text-xs text-destructive">{invalidMessage}</p>
      )}
      {showHint && (
        <p className="text-xs text-muted-foreground">
          {isFieldLocked && isLoading && loadingMessage
            ? loadingMessage
            : isEditMode && isLocked && lockedHintMessage
              ? lockedHintMessage
              : hintMessage}
        </p>
      )}
    </div>
  );
}
