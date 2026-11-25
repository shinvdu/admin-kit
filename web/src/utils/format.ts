import dayjs from 'dayjs';

/** Format an ISO 8601 timestamp for display in the current locale/timezone. */
export function formatDateTime(value: string | null | undefined) {
  if (!value) return '-';

  const date = dayjs(value);

  return date.isValid() ? date.format('YYYY-MM-DD HH:mm') : value;
}
