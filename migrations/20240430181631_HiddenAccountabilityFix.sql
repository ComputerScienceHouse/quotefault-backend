-- Add migration script here

ALTER TABLE public.quotes DROP COLUMN hidden;
ALTER TABLE public.hidden DROP CONSTRAINT hidden_reason_check;
