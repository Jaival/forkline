import { type InfiniteData, useMutation, useQueryClient } from "@tanstack/react-query";
import { commands, type Page, type RepoOverview, type RepoRow } from "../../bindings";

/** Pin or unpin a repo. The button flips at once; a failed write refetches. */
export function useSetPinned() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (v: { id: number; pinned: boolean }) => commands.repoSetPinned(v.id, v.pinned),
    onMutate: async ({ id, pinned }) => {
      await qc.cancelQueries({ queryKey: ["repos"] });
      qc.setQueryData<RepoOverview>(["repos", "overview", id], (o) => o && { ...o, pinned });
      qc.setQueriesData<InfiniteData<Page<RepoRow>>>({ queryKey: ["repos", "list"] }, (d) =>
        d && {
          ...d,
          pages: d.pages.map((p) => ({ ...p, items: p.items.map((r) => (r.id === id ? { ...r, pinned } : r)) })),
        },
      );
    },
    onError: () => qc.invalidateQueries({ queryKey: ["repos"] }),
  });
}
