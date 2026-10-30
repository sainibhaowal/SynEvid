'use client';

import { useEffect, useState } from 'react';
import { createPortal } from 'react-dom';

type TooltipPlacement = 'top' | 'right' | 'bottom' | 'left';

interface TooltipState {
  text: string;
  left: number;
  top: number;
  placement: TooltipPlacement;
}

interface ActiveDescription {
  element: HTMLElement;
  previous: string | null;
}

const TOOLTIP_ID = 'synevid-hover-tooltip';
const TOOLTIP_SELECTOR =
  '[data-tooltip], [title], button[aria-label], a[aria-label], [role="button"][aria-label]';

function findTooltipTarget(target: EventTarget | null): HTMLElement | null {
  if (!(target instanceof Element)) return null;
  const match = target.closest(TOOLTIP_SELECTOR);
  return match instanceof HTMLElement ? match : null;
}

function getAccessibleName(element: HTMLElement): string {
  return (
    element.getAttribute('aria-label')?.trim() ||
    element.getAttribute('aria-labelledby') ||
    element.textContent?.replace(/\s+/g, ' ').trim() ||
    ''
  );
}

function getTooltipText(element: HTMLElement): string | null {
  const title = element.getAttribute('title')?.trim();
  if (title) {
    element.setAttribute('data-tooltip', title);
    element.removeAttribute('title');

    const accessibleName = getAccessibleName(element);
    if (!accessibleName && element.matches('button, a, [role="button"]')) {
      element.setAttribute('aria-label', title);
    } else if (accessibleName !== title && !element.hasAttribute('aria-description')) {
      element.setAttribute('aria-description', title);
    }
  }

  return element.getAttribute('data-tooltip')?.trim() || element.getAttribute('aria-label')?.trim() || null;
}

function getTooltipPosition(element: HTMLElement, text: string): TooltipState | null {
  const rect = element.getBoundingClientRect();
  if (
    rect.width === 0 ||
    rect.height === 0 ||
    rect.bottom < 0 ||
    rect.top > window.innerHeight ||
    rect.right < 0 ||
    rect.left > window.innerWidth
  ) {
    return null;
  }

  const width = Math.min(280, Math.max(64, text.length * 6.7 + 20));
  const height = text.length > 36 ? 44 : 32;
  const gap = 8;
  const centerY = rect.top + rect.height / 2;
  const centerFits = centerY >= height / 2 + 8 && centerY <= window.innerHeight - height / 2 - 8;
  let placement: TooltipPlacement;

  if (rect.left < 76 && rect.right + width + gap <= window.innerWidth - 8 && centerFits) {
    placement = 'right';
  } else if (rect.right > window.innerWidth - 76 && rect.left - width - gap >= 8 && centerFits) {
    placement = 'left';
  } else if (rect.bottom + gap + height <= window.innerHeight - 8) {
    placement = 'bottom';
  } else {
    placement = 'top';
  }

  const centerX = rect.left + rect.width / 2;
  const clampedCenterX = Math.min(
    window.innerWidth - width / 2 - 8,
    Math.max(width / 2 + 8, centerX)
  );

  if (placement === 'right') {
    return { text, left: rect.right + gap, top: centerY, placement };
  }
  if (placement === 'left') {
    return { text, left: rect.left - gap, top: centerY, placement };
  }
  if (placement === 'bottom') {
    return { text, left: clampedCenterX, top: rect.bottom + gap, placement };
  }
  return { text, left: clampedCenterX, top: Math.max(height + 8, rect.top - gap), placement };
}

export default function TooltipLayer() {
  const [tooltip, setTooltip] = useState<TooltipState | null>(null);

  useEffect(() => {
    let pointerTarget: HTMLElement | null = null;
    let focusedTarget: HTMLElement | null = null;
    let pendingTarget: HTMLElement | null = null;
    let currentTarget: HTMLElement | null = null;
    let pendingTimer: number | null = null;
    let activeDescription: ActiveDescription | null = null;

    const clearPending = (target?: HTMLElement) => {
      if (target && pendingTarget !== target) return;
      if (pendingTimer !== null) window.clearTimeout(pendingTimer);
      pendingTimer = null;
      pendingTarget = null;
    };

    const restoreDescription = () => {
      if (!activeDescription) return;
      if (activeDescription.previous === null) {
        activeDescription.element.removeAttribute('aria-describedby');
      } else {
        activeDescription.element.setAttribute('aria-describedby', activeDescription.previous);
      }
      activeDescription = null;
    };

    const hideCurrent = (target?: HTMLElement) => {
      if (target && currentTarget !== target && pendingTarget !== target) return;
      clearPending(target);
      restoreDescription();
      currentTarget = null;
      setTooltip(null);
    };

    const show = (element: HTMLElement, delay: number) => {
      const text = getTooltipText(element);
      if (!text) return;

      clearPending();
      if (currentTarget && currentTarget !== element && focusedTarget !== currentTarget) {
        hideCurrent(currentTarget);
      }

      pendingTarget = element;
      pendingTimer = window.setTimeout(() => {
        if (pendingTarget !== element || !element.isConnected) return;
        pendingTarget = null;
        pendingTimer = null;

        if (currentTarget !== element) {
          restoreDescription();
          const accessibleName = getAccessibleName(element);
          if (accessibleName !== text && !element.hasAttribute('aria-description')) {
            const previous = element.getAttribute('aria-describedby');
            const describedBy = new Set((previous || '').split(/\s+/).filter(Boolean));
            describedBy.add(TOOLTIP_ID);
            element.setAttribute('aria-describedby', [...describedBy].join(' '));
            activeDescription = { element, previous };
          }
        }

        currentTarget = element;
        setTooltip(getTooltipPosition(element, text));
      }, delay);
    };

    const handlePointerOver = (event: Event) => {
      const element = findTooltipTarget(event.target);
      if (!element || element === findTooltipTarget((event as PointerEvent).relatedTarget)) return;
      pointerTarget = element;
      show(element, 280);
    };

    const handlePointerOut = (event: Event) => {
      const element = findTooltipTarget(event.target);
      if (!element || element === findTooltipTarget((event as PointerEvent).relatedTarget)) return;
      if (pointerTarget === element) pointerTarget = null;
      if (focusedTarget !== element) hideCurrent(element);
    };

    const handleFocusIn = (event: Event) => {
      const element = findTooltipTarget(event.target);
      if (!element) return;
      focusedTarget = element;
      show(element, 120);
    };

    const handleFocusOut = (event: Event) => {
      const element = findTooltipTarget(event.target);
      if (!element || element === findTooltipTarget((event as FocusEvent).relatedTarget)) return;
      if (focusedTarget === element) focusedTarget = null;
      if (pointerTarget !== element) hideCurrent(element);
    };

    const handleViewportChange = () => {
      if (!currentTarget) return;
      const text = getTooltipText(currentTarget);
      if (!text) {
        hideCurrent(currentTarget);
        return;
      }
      setTooltip(getTooltipPosition(currentTarget, text));
    };

    document.addEventListener('pointerover', handlePointerOver, true);
    document.addEventListener('pointerout', handlePointerOut, true);
    document.addEventListener('focusin', handleFocusIn, true);
    document.addEventListener('focusout', handleFocusOut, true);
    window.addEventListener('scroll', handleViewportChange, true);
    window.addEventListener('resize', handleViewportChange);

    return () => {
      document.removeEventListener('pointerover', handlePointerOver, true);
      document.removeEventListener('pointerout', handlePointerOut, true);
      document.removeEventListener('focusin', handleFocusIn, true);
      document.removeEventListener('focusout', handleFocusOut, true);
      window.removeEventListener('scroll', handleViewportChange, true);
      window.removeEventListener('resize', handleViewportChange);
      if (pendingTimer !== null) window.clearTimeout(pendingTimer);
      restoreDescription();
    };
  }, []);

  if (!tooltip || typeof document === 'undefined') return null;

  return createPortal(
    <div
      id={TOOLTIP_ID}
      className="synevid-tooltip"
      data-placement={tooltip.placement}
      role="tooltip"
      style={{ left: tooltip.left, top: tooltip.top }}
    >
      {tooltip.text}
    </div>,
    document.body
  );
}
