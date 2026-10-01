import { invoke } from "@/shared/api";
import type { ModelCatalogInfo, StandardModel } from "./types";

declare module "@/shared/api/tauri" {
  interface CommandMap {
    list_standard_models: { args: void; result: StandardModel[] };
    get_model_catalog_info: { args: void; result: ModelCatalogInfo };
    save_standard_model: { args: { model: StandardModel }; result: void };
    delete_standard_model: { args: { id: string }; result: void };
  }
}

export const modelApi = {
  list: () => invoke("list_standard_models"),
  catalogInfo: () => invoke("get_model_catalog_info"),
  save: (model: StandardModel) => invoke("save_standard_model", { model }),
  remove: (id: string) => invoke("delete_standard_model", { id }),
};
