export type StepProps = {
  /** 保存成功后调用：把当前步骤标记为 configured 并进入下一步。 */
  onComplete: () => void | Promise<void>;
  /** 可跳过步骤的跳过入口。 */
  onSkip: () => void | Promise<void>;
  /** 这是“登录失效后的重新认证”步骤，不写初始化设置状态。 */
  reauth?: boolean;
};
