import { useNavigationStore } from "./store";

export { useNavigationStore, type EditorPage, type MainPage, type Page } from "./store";
export { useEditorGuard } from "./useEditorGuard";

/** 页面组件常用的导航动作与当前页（不含离散状态，避免无谓重渲染）。 */
export function useNavigation() {
  const page = useNavigationStore((state) => state.page);
  const navigate = useNavigationStore((state) => state.navigate);
  const back = useNavigationStore((state) => state.back);
  const close = useNavigationStore((state) => state.close);
  const openEditor = useNavigationStore((state) => state.openEditor);
  const goTo = useNavigationStore((state) => state.goTo);
  return { page, navigate, back, close, openEditor, goTo };
}
