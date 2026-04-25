; Rovers domain — simplified IPC Rovers, STRIPS-only subset
; Source: IPC 2002 Rovers domain, heavily simplified
(define (domain rovers-small)
    (:requirements :strips :typing)
    (:types rover waypoint mode sample objective)
    (:predicates (at ?r - rover ?w - waypoint)
                 (can-travel ?w1 ?w2 - waypoint)
                 (available ?s - sample ?w - waypoint)
                 (have-sample ?s - sample)
                 (have-image ?o - objective)
                 (calibrated ?r - rover ?m - mode ?w - waypoint)
                 (supports ?m - mode ?w - waypoint)
                 (on-board-camera ?r - rover ?m - mode))

    (:action move-rover
        :parameters (?r - rover ?from ?to - waypoint)
        :precondition (and (at ?r ?from) (can-travel ?from ?to))
        :effect (and (at ?r ?to) (not (at ?r ?from))))

    (:action sample-rock
        :parameters (?r - rover ?w - waypoint ?s - sample)
        :precondition (and (at ?r ?w) (available ?s ?w))
        :effect (and (have-sample ?s) (not (available ?s ?w))))

    (:action calibrate
        :parameters (?r - rover ?w - waypoint ?m - mode)
        :precondition (and (at ?r ?w) (supports ?m ?w) (on-board-camera ?r ?m))
        :effect (calibrated ?r ?m ?w))

    (:action take-image
        :parameters (?r - rover ?w - waypoint ?m - mode ?o - objective)
        :precondition (and (at ?r ?w) (calibrated ?r ?m ?w) (supports ?m ?w))
        :effect (have-image ?o))
)

(define (problem rovers-small-1)
    (:domain rovers-small)
    (:objects rover1 - rover
              waypoint1 waypoint2 - waypoint
              rock1 - sample
              obj1 - objective
              infrared - mode)
    (:init (at rover1 waypoint1)
           (can-travel waypoint1 waypoint2) (can-travel waypoint2 waypoint1)
           (available rock1 waypoint2)
           (supports infrared waypoint2)
           (on-board-camera rover1 infrared))
    (:goal (and (have-sample rock1) (have-image obj1)))
)
