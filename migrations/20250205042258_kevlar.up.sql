-- Add migration script here

CREATE TABLE public.kevlar (
  uid varchar UNIQUE NOT NULL,
  enabled boolean NOT NULL DEFAULT false,
  last_modified timestamp NOT NULL DEFAULT NOW(),
  CONSTRAINT pkey PRIMARY KEY (uid)
);
