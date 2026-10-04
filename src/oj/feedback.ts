import { toast } from "@heroui/react";
export function notifySuccess(message: string) {
  return toast.success(message, { timeout: 3000 });
}
