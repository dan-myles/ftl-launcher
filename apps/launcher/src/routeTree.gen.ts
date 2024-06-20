/* prettier-ignore-start */

/* eslint-disable */

// @ts-nocheck

// noinspection JSUnusedGlobalSymbols

// This file is auto-generated by TanStack Router

// Import Routes

import { Route as rootRoute } from './routes/__root'
import { Route as IndexImport } from './routes/index'
import { Route as SettingsIndexImport } from './routes/settings/index'
import { Route as ServerBrowserIndexImport } from './routes/server-browser/index'
import { Route as ModManagerIndexImport } from './routes/mod-manager/index'

// Create/Update Routes

const IndexRoute = IndexImport.update({
  path: '/',
  getParentRoute: () => rootRoute,
} as any)

const SettingsIndexRoute = SettingsIndexImport.update({
  path: '/settings/',
  getParentRoute: () => rootRoute,
} as any)

const ServerBrowserIndexRoute = ServerBrowserIndexImport.update({
  path: '/server-browser/',
  getParentRoute: () => rootRoute,
} as any)

const ModManagerIndexRoute = ModManagerIndexImport.update({
  path: '/mod-manager/',
  getParentRoute: () => rootRoute,
} as any)

// Populate the FileRoutesByPath interface

declare module '@tanstack/react-router' {
  interface FileRoutesByPath {
    '/': {
      id: '/'
      path: '/'
      fullPath: '/'
      preLoaderRoute: typeof IndexImport
      parentRoute: typeof rootRoute
    }
    '/mod-manager/': {
      id: '/mod-manager/'
      path: '/mod-manager'
      fullPath: '/mod-manager'
      preLoaderRoute: typeof ModManagerIndexImport
      parentRoute: typeof rootRoute
    }
    '/server-browser/': {
      id: '/server-browser/'
      path: '/server-browser'
      fullPath: '/server-browser'
      preLoaderRoute: typeof ServerBrowserIndexImport
      parentRoute: typeof rootRoute
    }
    '/settings/': {
      id: '/settings/'
      path: '/settings'
      fullPath: '/settings'
      preLoaderRoute: typeof SettingsIndexImport
      parentRoute: typeof rootRoute
    }
  }
}

// Create and export the route tree

export const routeTree = rootRoute.addChildren({
  IndexRoute,
  ModManagerIndexRoute,
  ServerBrowserIndexRoute,
  SettingsIndexRoute,
})

/* prettier-ignore-end */

/* ROUTE_MANIFEST_START
{
  "routes": {
    "__root__": {
      "filePath": "__root.tsx",
      "children": [
        "/",
        "/mod-manager/",
        "/server-browser/",
        "/settings/"
      ]
    },
    "/": {
      "filePath": "index.tsx"
    },
    "/mod-manager/": {
      "filePath": "mod-manager/index.tsx"
    },
    "/server-browser/": {
      "filePath": "server-browser/index.tsx"
    },
    "/settings/": {
      "filePath": "settings/index.tsx"
    }
  }
}
ROUTE_MANIFEST_END */