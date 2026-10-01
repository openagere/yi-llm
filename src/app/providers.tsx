import { QueryClientProvider } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { createQueryClient } from "./queryClient";

const queryClient = createQueryClient();

/** 全局 Provider 组合：设置与导航由模块级 zustand store 承载，这里只需要 QueryClient。 */
export function AppProviders({ children }: { children: ReactNode }) {
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
}
