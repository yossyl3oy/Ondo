import * as Sentry from "@sentry/react";

export function initSentry() {
  // Only initialize in production
  if (import.meta.env.PROD) {
    Sentry.init({
      dsn: import.meta.env.VITE_SENTRY_DSN || "",
      environment: import.meta.env.MODE,
      // Performance monitoring
      tracesSampleRate: 0.1,
      // Only send errors, not all transactions
      beforeSend(event) {
        // Don't send events if DSN is not configured
        if (!import.meta.env.VITE_SENTRY_DSN) {
          return null;
        }
        return event;
      },
      // Additional context
      initialScope: {
        tags: {
          app: "ondo",
        },
      },
    });
  }
}

export { Sentry };
