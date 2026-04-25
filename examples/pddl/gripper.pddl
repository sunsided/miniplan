(define (domain gripper)
    (:requirements :strips :typing)
    (:types room ball gripper)
    (:predicates (at-robby ?r - room)
                 (at ?b - ball ?r - room)
                 (free ?g - gripper)
                 (carry ?b - ball ?g - gripper))

    (:action move
        :parameters (?from ?to - room)
        :precondition (at-robby ?from)
        :effect (and (at-robby ?to) (not (at-robby ?from))))

    (:action pick
        :parameters (?obj - ball ?room - room ?gripper - gripper)
        :precondition (and (at ?obj ?room) (at-robby ?room) (free ?gripper))
        :effect (and (carry ?obj ?gripper)
                     (not (free ?gripper))
                     (not (at ?obj ?room))))

    (:action drop
        :parameters (?obj - ball ?room - room ?gripper - gripper)
        :precondition (and (carry ?obj ?gripper) (at-robby ?room))
        :effect (and (free ?gripper)
                     (at ?obj ?room)
                     (not (carry ?obj ?gripper))))
)

(define (problem gripper-4)
    (:domain gripper)
    (:objects rooma roomb - room
              ball1 ball2 ball3 ball4 - ball
              left right - gripper)
    (:init (at-robby rooma)
           (at ball1 rooma) (at ball2 rooma)
           (at ball3 rooma) (at ball4 rooma)
           (free left) (free right))
    (:goal (and (at ball1 roomb) (at ball2 roomb)
                (at ball3 roomb) (at ball4 roomb)))
)
