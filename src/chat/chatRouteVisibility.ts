import { createContext, useContext } from 'react'

// Native input listeners must ignore routes retained only for background work.
export const RouteActiveContext = createContext(true)
export const useChatRouteActive = () => useContext(RouteActiveContext)
