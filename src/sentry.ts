import * as Sentry from "@sentry/react";

export function initSentry() {
  const dsn = import.meta.env.VITE_SENTRY_DSN;

  if (dsn) {
    Sentry.init({
      dsn,
      environment: import.meta.env.MODE,
      tracesSampleRate: 0.1,
      initialScope: {
        tags: {
          app: "ondo",
        },
      },
    });
  }
}

export function testSentryError() {
  Sentry.captureException(new Error("Sentry test error from Ondo"));
  console.log("[Sentry] Test error sent");
}

export { Sentry };
