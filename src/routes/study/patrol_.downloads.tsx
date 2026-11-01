import { createFileRoute } from "@tanstack/react-router";
import DownloadManagerPage from "../../patrol/DownloadManagerPage";

export const Route = createFileRoute("/study/patrol_/downloads")({
  head: () => ({ meta: [{ title: "录像下载管理" }] }),
  component: DownloadManagerPage,
});
