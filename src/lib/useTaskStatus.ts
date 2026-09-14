import { useEffect, useRef, useState } from "react";

export function useTaskStatus<T>(key: string, read: () => Promise<T | null>) {
  const [task, setTask] = useState<T | null>(null);
  const [initializing, setInitializing] = useState(true);
  const [statusError, setStatusError] = useState("");
  const reader = useRef(read);
  reader.current = read;
  const mutation = useRef(false);
  const revision = useRef(0);
  useEffect(() => {
    let alive = true;
    let polling = false;
    setTask(null);
    setInitializing(true);
    setStatusError("");
    const poll = async () => {
      if (polling || mutation.current) return;
      polling = true;
      const current = revision.current;
      try {
        const value = await reader.current();
        if (alive && !mutation.current && current === revision.current) {
          setTask(value);
          setInitializing(false);
          setStatusError("");
        }
      } catch (e) {
        if (alive) setStatusError(String(e));
      } finally {
        polling = false;
      }
    };
    void poll();
    const timer = setInterval(poll, 1000);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, [key]);
  return {
    task,
    setTask,
    initializing,
    statusError,
    beginMutation: () => {
      mutation.current = true;
      revision.current += 1;
    },
    endMutation: () => {
      mutation.current = false;
    },
  };
}
