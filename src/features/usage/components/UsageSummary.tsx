import { Activity, Boxes, Sigma } from "lucide-react";
import { useI18n } from "@/shared/i18n";
import type { UsageSnapshot } from "../types";
import { UsageNumber, UsageTokenMetrics } from "./UsageCommon";

export function UsageSummary({ snapshot }: { snapshot: UsageSnapshot }) {
  const { t } = useI18n();
  const total = snapshot.input_tokens + snapshot.output_tokens;
  return <section className="analytics-summary" aria-label={t("usage.summary.label")}>
    <div className="analytics-summary-tokens">
      <div className="analytics-total-metric"><span><Sigma size={15} />{t("usage.common.totalTokens")}</span><strong><UsageNumber value={total} /></strong><small>{t("usage.summary.inputPlusOutput")}</small></div>
      <UsageTokenMetrics values={snapshot} />
    </div>
    <div className="analytics-summary-secondary"><span><Activity size={13} />{t("usage.summary.requests")}<strong><UsageNumber value={snapshot.total_requests} /></strong></span><span><Boxes size={13} />{t("usage.summary.activeModels")}<strong><UsageNumber value={snapshot.models.length} /></strong></span><span>{t("usage.summary.average")}<strong><UsageNumber value={snapshot.total_requests ? Math.round(total / snapshot.total_requests) : 0} /></strong><small>Token</small></span></div>
  </section>;
}
