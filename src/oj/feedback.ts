import { toast } from "../ui";
export function notifySuccess(message: string) {
  return toast.success(message, { timeout: 3000 });
}
