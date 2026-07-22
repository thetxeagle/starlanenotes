import { Component, type ErrorInfo, type ReactNode } from "react";
import { AlertTriangle } from "lucide-react";

interface Props {
  children: ReactNode;
}

interface State {
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("[ErrorBoundary]", error, info.componentStack);
  }

  render() {
    if (this.state.error) {
      return (
        <div className="flex h-full items-center justify-center">
          <div className="max-w-sm text-center">
            <AlertTriangle size={24} className="mx-auto mb-3 text-text-muted" />
            <p className="text-sm font-medium text-text">Something went wrong</p>
            <p className="mt-1 text-xs text-text-muted">{this.state.error.message}</p>
            <button
              onClick={() => this.setState({ error: null })}
              className="mt-4 rounded-button border border-border px-3 py-1.5 text-xs text-text-muted transition-colors duration-100 hover:bg-surface-raised hover:text-text"
            >
              Try again
            </button>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}
