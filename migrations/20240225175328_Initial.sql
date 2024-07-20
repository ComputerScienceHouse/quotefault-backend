--
-- PostgreSQL database dump
--

-- Dumped from database version 13.13 (Debian 13.13-0+deb11u1)
-- Dumped by pg_dump version 13.13 (Debian 13.13-0+deb11u1)

SET client_encoding = 'UTF8';

--
-- Name: vote; Type: TYPE; Schema: public; Owner: postgres
--

CREATE TYPE public.vote AS ENUM (
    'upvote',
    'downvote'
);


--
-- Name: favorites; Type: TABLE; Schema: public; Owner: quotefault
--

CREATE TABLE public.favorites (
    quote_id integer NOT NULL,
    username character varying(32) NOT NULL
);


--
-- Name: quotes; Type: TABLE; Schema: public; Owner: quotefault
--

CREATE TABLE public.quotes (
    id integer NOT NULL,
    submitter character varying(32) NOT NULL,
    "timestamp" timestamp without time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    hidden boolean DEFAULT false NOT NULL
);


--
-- Name: quotes_id_seq; Type: SEQUENCE; Schema: public; Owner: quotefault
--

ALTER TABLE public.quotes ALTER COLUMN id ADD GENERATED ALWAYS AS IDENTITY (
    SEQUENCE NAME public.quotes_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: reports; Type: TABLE; Schema: public; Owner: quotefault
--

CREATE TABLE public.reports (
    id integer NOT NULL,
    quote_id integer NOT NULL,
    reason text NOT NULL,
    submitter_hash bytea NOT NULL,
    "timestamp" timestamp without time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    resolver character varying(32)
);


--
-- Name: reports_id_seq; Type: SEQUENCE; Schema: public; Owner: quotefault
--

ALTER TABLE public.reports ALTER COLUMN id ADD GENERATED ALWAYS AS IDENTITY (
    SEQUENCE NAME public.reports_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: shards; Type: TABLE; Schema: public; Owner: quotefault
--

CREATE TABLE public.shards (
    quote_id integer NOT NULL,
    index smallint NOT NULL,
    body text NOT NULL,
    speaker character varying(32) NOT NULL
);


--
-- Name: votes; Type: TABLE; Schema: public; Owner: quotefault
--

CREATE TABLE public.votes (
    quote_id integer NOT NULL,
    vote public.vote NOT NULL,
    submitter character varying(32) NOT NULL,
    "timestamp" timestamp without time zone DEFAULT CURRENT_TIMESTAMP NOT NULL
);


--
-- Name: favorites favorites_pkey; Type: CONSTRAINT; Schema: public; Owner: quotefault
--

ALTER TABLE ONLY public.favorites
    ADD CONSTRAINT favorites_pkey PRIMARY KEY (quote_id, username);


--
-- Name: quotes quotes_pkey; Type: CONSTRAINT; Schema: public; Owner: quotefault
--

ALTER TABLE ONLY public.quotes
    ADD CONSTRAINT quotes_pkey PRIMARY KEY (id);


--
-- Name: reports reports_pkey; Type: CONSTRAINT; Schema: public; Owner: quotefault
--

ALTER TABLE ONLY public.reports
    ADD CONSTRAINT reports_pkey PRIMARY KEY (quote_id, submitter_hash);


--
-- Name: shards shards_pkey; Type: CONSTRAINT; Schema: public; Owner: quotefault
--

ALTER TABLE ONLY public.shards
    ADD CONSTRAINT shards_pkey PRIMARY KEY (quote_id, index);


--
-- Name: votes votes_pkey; Type: CONSTRAINT; Schema: public; Owner: quotefault
--

ALTER TABLE ONLY public.votes
    ADD CONSTRAINT votes_pkey PRIMARY KEY (quote_id, submitter);


--
-- Name: favorites favorites_quote_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: quotefault
--

ALTER TABLE ONLY public.favorites
    ADD CONSTRAINT favorites_quote_id_fkey FOREIGN KEY (quote_id) REFERENCES public.quotes(id) ON DELETE CASCADE;


--
-- Name: reports reports_quote_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: quotefault
--

ALTER TABLE ONLY public.reports
    ADD CONSTRAINT reports_quote_id_fkey FOREIGN KEY (quote_id) REFERENCES public.quotes(id) ON DELETE CASCADE;


--
-- Name: shards shards_quote_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: quotefault
--

ALTER TABLE ONLY public.shards
    ADD CONSTRAINT shards_quote_id_fkey FOREIGN KEY (quote_id) REFERENCES public.quotes(id) ON DELETE CASCADE;


--
-- Name: votes votes_quote_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: quotefault
--

ALTER TABLE ONLY public.votes
    ADD CONSTRAINT votes_quote_id_fkey FOREIGN KEY (quote_id) REFERENCES public.quotes(id) ON DELETE CASCADE;


--
-- PostgreSQL database dump complete
--

