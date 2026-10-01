import { QueryCache, QueryClient } from "@tanstack/react-query";
import { describeError } from "@/shared/api";
import { translate, type MessageKey } from "@/shared/i18n";
import { notify } from "@/shared/notify";

declare module "@tanstack/react-query" {
  interface Register {
    queryMeta: {
      /** 查询失败时的提示前缀（i18n 键）；未设置则不弹出全局提示。 */
      errorPrefix?: MessageKey;
    };
  }
}

export function createQueryClient(): QueryClient {
  return new QueryClient({
    queryCache: new QueryCache({
      onError: (error, query) => {
        const prefix = query.meta?.errorPrefix;
        if (prefix) notify(`${translate(prefix)}${describeError(error)}`, "error");
      },
    }),
    defaultOptions: {
      queries: {
        staleTime: 10_000,
        retry: false,
        refetchOnWindowFocus: true,
      },
    },
  });
}
