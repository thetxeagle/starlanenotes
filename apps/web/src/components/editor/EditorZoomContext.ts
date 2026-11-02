import { createContext } from "react";

export const EditorZoomContext = createContext<{ cssVar: number }>({ cssVar: 1 });
