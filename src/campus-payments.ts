export const WECHAT_SERVICES = [
  {
    id: "electricity",
    title: "电费",
    description: "前往官方微信服务办理宿舍电费。",
    url: "https://weixiao.qq.com/apps/public/service/index.html?hid=bf11210a3cc169eaa981beeb476596f6&media_id=gh_a280c83dc0be",
  },
  {
    id: "campus-card",
    title: "校园卡",
    description: "前往官方微信服务充值或查询校园卡。",
    url: "https://ecardwx.szu.edu.cn/wechat/home/index.html",
  },
] as const;

export type NetworkFeePortalNotice = {
  code:
    | "loadTimeout"
    | "loginTimeout"
    | "formTimeout"
    | "navigationFailed"
    | "credentialsUnavailable";
  message: string;
};
