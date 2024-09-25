select
    pq.id as "id!",
    s.index as "index!",
    pq.submitter as "submitter!",
    pq.timestamp as "timestamp!",
    s.body as "body!",
    s.speaker as "speaker!",
    hidden.reason as "hidden_reason: Option<String>",
    hidden.actor as "hidden_actor: Option<String>",
    v.vote as "vote: Option<Vote>",
    (case when pq.score is null then 0 else pq.score end) as "score!",
    (case when f.username is null then false else true end) as "favorited!"
from
    (
        select *
        from
            (
                select
                    id,
                    submitter,
                    timestamp,
                    (case when quote_id is not null then true else false end) as hidden
                from quotes as _q
                left join (select quote_id from hidden) _h on _q.id = _h.quote_id
            ) as q
        left join
            (
                select
                    quote_id,
                    sum(
                        case
                            when vote = 'upvote'
                            then 1
                            when vote = 'downvote'
                            then -1
                            else 0
                        end
                    ) as score
                from votes
                group by quote_id
            ) as t
            on t.quote_id = q.id
        where
            case
                when $7 and $6 and $9
                then q.hidden
                when $7 and $6
                then
                    case
                        when
                            (
                                q.submitter = $8
                                or $8
                                in (select speaker from shards where quote_id = q.id)
                            )
                        then q.hidden
                        else false
                    end
                when $7 and not $6
                then not q.hidden
                else
                    (
                        case
                            when
                                q.hidden
                                and (
                                    q.submitter = $8
                                    or $8 in (
                                        select speaker from shards where quote_id = q.id
                                    )
                                )
                            then q.hidden
                            else not q.hidden
                        end
                    )
            end
            and case when $2::int4 > 0 then q.id < $2::int4 else true end
            and submitter like $5
            and (
                submitter like $10
                or q.id in (select quote_id from shards s where speaker like $10)
            )
            and q.id
            in (select quote_id from shards where body ilike $3 and speaker like $4)
            and case
                when $11
                then q.id in (select quote_id from favorites where username = $8)
                else true
            end
        order by
            (
                case
                    when $12::bool and $13::bool
                    then score
                    when $12::bool and not $13::bool
                    then -1 * score
                    when not $12::bool and $13::bool
                    then extract(epoch from timestamp)
                    when not $12::bool and not $13::bool
                    then -1 * extract(epoch from timestamp)
                end
            ),
            q.id desc
        limit $1
    ) as pq
left join hidden on hidden.quote_id = pq.id
left join shards s on s.quote_id = pq.id
left join
    (select quote_id, vote from votes where submitter = $8) v on v.quote_id = pq.id
left join
    (select quote_id, username from favorites where username = $8) f
    on f.quote_id = pq.id
order by timestamp desc, pq.id desc, s.index
