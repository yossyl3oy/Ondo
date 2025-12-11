import { useState, useEffect, useCallback } from "react";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { captureUpdateError } from "../sentry";

export interface UpdateInfo {
  available: boolean;
  version?: string;
  currentVersion?: string;
}

export interface UpdaterState {
  checking: boolean;
  updateInfo: UpdateInfo | null;
  downloading: boolean;
  progress: number;
  error: string | null;
}

export function useUpdater() {
  const [state, setState] = useState<UpdaterState>({
    checking: false,
    updateInfo: null,
    downloading: false,
    progress: 0,
    error: null,
  });

  const checkForUpdate = useCallback(async () => {
    setState((prev) => ({ ...prev, checking: true, error: null }));
    console.log("[Updater] Checking for updates...");

    try {
      const update = await check();
      console.log("[Updater] Check result:", update);

      if (update) {
        console.log("[Updater] Update available:", update.version);
        setState((prev) => ({
          ...prev,
          checking: false,
          updateInfo: {
            available: true,
            version: update.version,
            currentVersion: update.currentVersion,
          },
        }));
        return update;
      } else {
        console.log("[Updater] No update available");
        setState((prev) => ({
          ...prev,
          checking: false,
          updateInfo: { available: false },
        }));
        return null;
      }
    } catch (error) {
      console.error("[Updater] Error:", error);
      // Get detailed error information
      let errorMessage = "Update check failed";
      if (error instanceof Error) {
        errorMessage = error.message;
        // Include stack trace if available
        if (error.stack) {
          console.error("[Updater] Stack:", error.stack);
        }
      } else if (typeof error === "string") {
        errorMessage = error;
      } else if (error && typeof error === "object") {
        errorMessage = JSON.stringify(error);
      }
      captureUpdateError(`check: ${errorMessage}`);
      setState((prev) => ({
        ...prev,
        checking: false,
        error: errorMessage,
      }));
      return null;
    }
  }, []);

  const downloadAndInstall = useCallback(async () => {
    setState((prev) => ({ ...prev, downloading: true, progress: 0, error: null }));

    try {
      const update = await check();
      if (!update) {
        setState((prev) => ({
          ...prev,
          downloading: false,
          error: "No update available",
        }));
        return;
      }

      let downloaded = 0;
      let contentLength = 0;

      await update.downloadAndInstall((event) => {
        switch (event.event) {
          case "Started":
            contentLength = event.data.contentLength ?? 0;
            break;
          case "Progress":
            downloaded += event.data.chunkLength;
            if (contentLength > 0) {
              const progress = Math.round((downloaded / contentLength) * 100);
              setState((prev) => ({ ...prev, progress }));
            }
            break;
          case "Finished":
            setState((prev) => ({ ...prev, progress: 100 }));
            break;
        }
      });

      // Relaunch the app after update
      await relaunch();
    } catch (error) {
      console.error("[Updater] Download error:", error);
      // Get detailed error information
      let errorMessage = "Update failed";
      if (error instanceof Error) {
        errorMessage = error.message;
        if (error.stack) {
          console.error("[Updater] Stack:", error.stack);
        }
      } else if (typeof error === "string") {
        errorMessage = error;
      } else if (error && typeof error === "object") {
        errorMessage = JSON.stringify(error);
      }
      captureUpdateError(`download: ${errorMessage}`);
      setState((prev) => ({
        ...prev,
        downloading: false,
        error: errorMessage,
      }));
    }
  }, []);

  // Check for updates on mount (with delay to not interfere with boot)
  useEffect(() => {
    const timer = setTimeout(() => {
      checkForUpdate();
    }, 10000); // Check 10 seconds after launch

    return () => clearTimeout(timer);
  }, [checkForUpdate]);

  return {
    ...state,
    checkForUpdate,
    downloadAndInstall,
  };
}
